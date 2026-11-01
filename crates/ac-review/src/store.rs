//! A training set as the review reads it: the takes (names, labels, where each starts), every
//! frame's root offset, and the rotations from the review cache (see [`crate::npz`]).
//!
//! The cache is `<data>/.review-cache/<set>.rot`: the npz's rotations as raw half floats, made once
//! per set and made again when the npz changes (its size and time are kept beside it).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// How the problem of an empty data folder starts; a library gives the review something to show.
pub const NO_RIG_TAKES: &str = "no rig takes";

use ac_core::quat::{Quat, Vec3};
use ac_io::library::{Kind, Library};
use serde_json::Value;

use crate::npz;

type R<T> = Result<T, String>;

/// Text arrays bigger than this are not take names (a damaged file).
const MAX_SMALL: usize = 512 << 20;
/// A take this long (frames) is refused rather than read into memory (19 hours at 30 fps).
const MAX_TAKE: usize = 2_000_000;

#[derive(Debug, Clone)]
pub struct Take {
    pub name: String,
    /// The set's own words for it (CMU's description, a 100STYLE style); else the name.
    pub label: String,
    /// Its first frame in the set.
    pub start: usize,
    pub frames: usize,
    /// Its own rate: a library's clips differ, a set's takes share the set's.
    pub fps: f64,
}

/// Where a library group's clips are, and the rig's bones in order.
#[derive(Debug, Clone)]
pub struct LibraryGroup {
    pub dir: PathBuf,
    rig: Arc<Vec<String>>,
}

#[derive(Debug)]
pub struct Set {
    pub name: String,
    pub npz: PathBuf,
    pub takes: Vec<Take>,
    pub bones: usize,
    pub frames: usize,
    pub fps: f64,
    /// CRC-32 of the rig file the set was retargeted onto (`ac_train/stamp.py`); None for a set
    /// built before stamps.
    pub rig_crc32: Option<u32>,
    /// A group of a person's library (`library:<group>`), read from its `.clip.json` files; none:
    /// a retargeted npz.
    pub library: Option<LibraryGroup>,
    roots: Vec<[f32; 3]>,
    cache: PathBuf,
}

/// One take, every frame: rotations (one per bone) and root offsets.
pub struct Motion {
    pub rotations: Vec<Vec<Quat>>,
    pub roots: Vec<Vec3>,
}

/// One frame of half floats to unit rotations.
fn decode_frame(raw: &[u8]) -> Vec<Quat> {
    raw.as_chunks::<8>()
        .0
        .iter()
        .map(|q| {
            let h = |k: usize| npz::f16(u16::from_le_bytes([q[2 * k], q[2 * k + 1]])) as f64;
            let v = [h(0), h(1), h(2), h(3)];
            let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2] + v[3] * v[3]).sqrt();
            if n > 1e-9 && n.is_finite() {
                [v[0] / n, v[1] / n, v[2] / n, v[3] / n]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            }
        })
        .collect()
}

/// A clip file's frames (`at`, or all of them), each pose's rotations in the rig's bone order. A
/// clip whose bones aren't the rig's is refused, as it couldn't be shown.
fn clip_frames(path: &Path, rig: &[String], at: Option<&[f64]>) -> R<Motion> {
    let say = |e: String| format!("{}: {e}", path.display());
    let text = std::fs::read_to_string(path).map_err(|e| say(e.to_string()))?;
    let clip = ac_core::clipfile::load_clip(&text).map_err(say)?;
    let frames: Vec<f64> = match at {
        Some(f) => f.to_vec(),
        None => {
            let n = clip.length();
            if n <= 0 {
                return Err(say("no frames".into()));
            }
            if n as usize > MAX_TAKE {
                return Err(say(format!("{n} frames: too long to show")));
            }
            (0..n).map(|f| f as f64).collect()
        }
    };
    let order = rig
        .iter()
        .map(|b| {
            clip.bones
                .iter()
                .position(|c| c == b)
                .ok_or_else(|| say(format!("no bone {b:?}: the clip isn't on this rig")))
        })
        .collect::<R<Vec<usize>>>()?;
    let (rots, roots) = clip.sample(&frames).map_err(say)?;
    let rotations = rots
        .iter()
        .map(|pose| {
            order
                .iter()
                .map(|&i| pose.get(i).copied().unwrap_or([0.0, 0.0, 0.0, 1.0]))
                .collect()
        })
        .collect();
    Ok(Motion { rotations, roots })
}

fn stamp_of(npz: &Path) -> R<String> {
    let m = std::fs::metadata(npz).map_err(|e| format!("{}: {e}", npz.display()))?;
    let t = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    Ok(format!("{} {t}", m.len()))
}

/// Reading a take's middle pose on another thread, for its thumbnail.
pub enum PoseJob {
    Cache {
        path: PathBuf,
        at: u64,
        len: usize,
        root: [f32; 3],
    },
    Clip {
        path: PathBuf,
        rig: Arc<Vec<String>>,
        frame: f64,
    },
}

impl PoseJob {
    pub fn run(self) -> R<(Vec<Quat>, Vec3)> {
        match self {
            PoseJob::Cache { path, at, len, root } => {
                let say = |e: std::io::Error| format!("{}: {e}", path.display());
                let mut f = File::open(&path).map_err(say)?;
                f.seek(SeekFrom::Start(at)).map_err(say)?;
                let mut raw = vec![0u8; len];
                f.read_exact(&mut raw).map_err(say)?;
                Ok((
                    decode_frame(&raw),
                    [root[0] as f64, root[1] as f64, root[2] as f64],
                ))
            }
            PoseJob::Clip { path, rig, frame } => {
                let m = clip_frames(&path, &rig, Some(&[frame]))?;
                let rots = m.rotations.into_iter().next();
                let root = m.roots.into_iter().next();
                rots.zip(root)
                    .ok_or_else(|| format!("{}: no frames", path.display()))
            }
        }
    }
}

impl Set {
    /// Read a set's takes and root offsets (the rotations stay in the npz until [`Set::unpack`]).
    pub fn open(npz_path: &Path) -> R<Set> {
        let name = npz_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .ok_or("an npz path has a file name")?;
        let (h, b) = npz::read(npz_path, "takes", MAX_SMALL)?;
        let names = npz::strings(&h, &b).map_err(|e| format!("{name}: takes: {e}"))?;
        let labels = match npz::read(npz_path, "labels", MAX_SMALL) {
            Ok((h, b)) => npz::strings(&h, &b).map_err(|e| format!("{name}: labels: {e}"))?,
            Err(_) => names.clone(), // older sets have no labels: the take name says it
        };
        let (h, b) = npz::read(npz_path, "starts", MAX_SMALL)?;
        let starts = npz::i64s(&h, &b).map_err(|e| format!("{name}: starts: {e}"))?;
        let (h, b) = npz::read(npz_path, "roots", usize::MAX >> 1)?;
        let flat = npz::f32s(&h, &b).map_err(|e| format!("{name}: roots: {e}"))?;
        let roots: Vec<[f32; 3]> = flat.as_chunks::<3>().0.to_vec();
        let fps = match npz::read(npz_path, "fps", 64) {
            Ok((h, b)) if h.descr == "<f8" && b.len() == 8 => {
                f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
            }
            _ => 30.0,
        };
        let rig_crc32 = match npz::read(npz_path, "rig_crc32", 64) {
            Ok((h, b)) => {
                let v = npz::i64s(&h, &b).map_err(|e| format!("{name}: rig_crc32: {e}"))?;
                Some(
                    v.first()
                        .and_then(|&c| u32::try_from(c).ok())
                        .ok_or(format!("{name}: rig_crc32 is not a CRC-32"))?,
                )
            }
            Err(_) => None,
        };
        let rh = npz::peek(npz_path, "rotations")?;
        let [frames, bones, 4] = rh.shape[..] else {
            return Err(format!(
                "{name}: rotations are {:?}, not (frames, bones, 4)",
                rh.shape
            ));
        };
        if rh.descr != "<f2" {
            return Err(format!("{name}: rotations are {}, not half floats", rh.descr));
        }
        if frames != roots.len() || names.len() != starts.len() || labels.len() != names.len() {
            return Err(format!(
                "{name}: {frames} rotation frames, {} root frames, {} takes, {} starts, {} labels",
                roots.len(),
                names.len(),
                starts.len(),
                labels.len()
            ));
        }
        let mut takes = Vec::with_capacity(names.len());
        for (i, (n, l)) in names.into_iter().zip(labels).enumerate() {
            let s = starts.get(i).copied().unwrap_or(0);
            let e = starts.get(i + 1).copied().unwrap_or(frames as i64);
            if s < 0 || e < s || e as usize > frames {
                return Err(format!("{name}: take {n} runs {s}..{e} in {frames} frames"));
            }
            takes.push(Take {
                name: n,
                label: l,
                start: s as usize,
                frames: (e - s) as usize,
                fps,
            });
        }
        let dir = npz_path.parent().unwrap_or(Path::new(".")).join(".review-cache");
        Ok(Set {
            cache: dir.join(format!("{name}.rot")),
            name,
            npz: npz_path.to_path_buf(),
            takes,
            bones,
            frames,
            fps,
            rig_crc32,
            library: None,
            roots,
        })
    }

    fn stamp_path(&self) -> PathBuf {
        self.cache.with_extension("stamp")
    }

    /// Whether the cache holds this npz's rotations.
    pub fn unpacked(&self) -> bool {
        if self.library.is_some() {
            return true; // read from its clip files, nothing to unpack
        }
        let size = (self.frames * self.bones * 4 * 2) as u64;
        let have = std::fs::read_to_string(self.stamp_path()).unwrap_or_default();
        stamp_of(&self.npz).is_ok_and(|s| s == have.trim())
            && std::fs::metadata(&self.cache).is_ok_and(|m| m.len() == size)
    }

    /// The work of unpacking the rotations into the cache, to run on another thread.
    pub fn unpack_job(&self) -> Unpack {
        Unpack {
            npz: self.npz.clone(),
            cache: self.cache.clone(),
            stamp: self.stamp_path(),
        }
    }

    pub fn take(&self, i: usize) -> R<&Take> {
        self.takes
            .get(i)
            .ok_or_else(|| format!("{} has no take {i}", self.name))
    }

    pub fn find(&self, name: &str) -> Option<usize> {
        self.takes.iter().position(|t| t.name == name)
    }

    /// How to read take `i`'s middle pose (from the cache: [`Set::unpacked`] first).
    pub fn pose_job(&self, i: usize) -> R<PoseJob> {
        let t = self.take(i)?;
        if let Some(g) = &self.library {
            return Ok(PoseJob::Clip {
                path: g.dir.join(format!("{}{}", t.name, Kind::Animations.ext())),
                rig: g.rig.clone(),
                frame: (t.frames / 2) as f64,
            });
        }
        let stride = self.bones * 4 * 2;
        let frame = t.start + t.frames / 2;
        Ok(PoseJob::Cache {
            path: self.cache.clone(),
            at: (frame * stride) as u64,
            len: stride,
            root: *self
                .roots
                .get(frame)
                .ok_or_else(|| format!("{}: roots end early", t.name))?,
        })
    }

    /// Every frame of take `i` (from the cache: [`Set::unpacked`] first).
    pub fn motion(&self, i: usize) -> R<Motion> {
        let t = self.take(i)?;
        if t.frames > MAX_TAKE {
            return Err(format!("{} is {} frames: too long to show", t.name, t.frames));
        }
        if let Some(g) = &self.library {
            return clip_frames(
                &g.dir.join(format!("{}{}", t.name, Kind::Animations.ext())),
                &g.rig,
                None,
            );
        }
        let stride = self.bones * 4 * 2;
        let mut f = File::open(&self.cache).map_err(|e| format!("{}: {e}", self.cache.display()))?;
        f.seek(SeekFrom::Start((t.start * stride) as u64))
            .map_err(|e| format!("{}: {e}", self.cache.display()))?;
        let mut raw = vec![0u8; t.frames * stride];
        f.read_exact(&mut raw)
            .map_err(|e| format!("{}: {e} (unpack the set again)", self.cache.display()))?;
        let rotations = raw.chunks_exact(stride).map(decode_frame).collect();
        let roots = self
            .roots
            .get(t.start..t.start + t.frames)
            .ok_or_else(|| format!("{}: roots end early", t.name))?
            .iter()
            .map(|r| [r[0] as f64, r[1] as f64, r[2] as f64])
            .collect();
        Ok(Motion { rotations, roots })
    }
}

/// Unpacking a set's rotations into the review cache (seconds to a minute a set; once).
pub struct Unpack {
    npz: PathBuf,
    cache: PathBuf,
    stamp: PathBuf,
}

impl Unpack {
    pub fn run(self) -> R<()> {
        if let Some(dir) = self.cache.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let stamp = stamp_of(&self.npz)?;
        // the stamp goes first so a half-written cache never looks finished
        std::fs::remove_file(&self.stamp).ok();
        npz::unpack(&self.npz, "rotations", &self.cache)?;
        craft_atomic::atomic_write(&self.stamp, stamp.as_bytes())
            .map_err(|e| format!("{}: {e}", self.stamp.display()))
    }
}

/// A library's groups as sets named `library:<group>`, from its index (a person's thousands of
/// clips aren't opened again; the library's own `.index` is brought up to date, the clips are never
/// changed). Clips that can't be read, are empty or are evaluation-only are left out and said.
pub fn open_library(root: &Path, rig: &[String]) -> (Vec<Set>, Vec<String>) {
    let lib = Library::new(root, Kind::Animations);
    let rig = Arc::new(rig.to_vec());
    let mut problems = Vec::new();
    let (mut evaluation, mut empty) = (0, 0);
    let mut groups: Vec<(String, Vec<Take>)> = Vec::new();
    for item in lib.listed() {
        let word = |k: &str| {
            item.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let (group, name) = (word("group"), word("name"));
        if let Some(e) = item.get("error").and_then(Value::as_str) {
            problems.push(format!("library clip {group}/{name}: {e}"));
            continue;
        }
        if item.get("evaluation_only").and_then(Value::as_bool) == Some(true) {
            evaluation += 1;
            continue;
        }
        let frames = item.get("frames").and_then(Value::as_i64).unwrap_or(0);
        if frames <= 0 {
            empty += 1;
            continue;
        }
        let fps = item.get("fps").and_then(Value::as_f64).unwrap_or(30.0);
        let take = Take {
            label: name.clone(),
            name,
            start: 0,
            frames: frames as usize,
            fps,
        };
        match groups.last_mut() {
            Some((g, takes)) if *g == group => takes.push(take),
            _ => groups.push((group, vec![take])),
        }
    }
    if evaluation > 0 {
        problems.push(format!(
            "{evaluation} evaluation-only library clips are left out: their licence doesn't allow training"
        ));
    }
    if empty > 0 {
        problems.push(format!("{empty} library clips have no frames and are left out"));
    }
    if groups.is_empty() {
        problems.push(format!(
            "no clips in {} (a library's clips are animations/<group>/<name>{})",
            lib.root.display(),
            Kind::Animations.ext()
        ));
    }
    let sets = groups
        .into_iter()
        .map(|(group, takes)| Set {
            name: format!("{}{group}", crate::training_set::LIBRARY),
            npz: PathBuf::new(),
            bones: rig.len(),
            frames: takes.iter().map(|t| t.frames).sum(),
            fps: 30.0,
            rig_crc32: None,
            library: Some(LibraryGroup {
                dir: lib.root.join(&group),
                rig: rig.clone(),
            }),
            takes,
            roots: Vec::new(),
            cache: PathBuf::new(),
        })
        .collect();
    (sets, problems)
}

/// Every set in a folder (`*.npz`), by name; a set that won't open is said, the rest still load.
pub fn open_all(dir: &Path) -> (Vec<Set>, Vec<String>) {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "npz"))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    let (mut sets, mut errors) = (Vec::new(), Vec::new());
    for p in paths {
        match Set::open(&p) {
            Ok(s) => sets.push(s),
            Err(e) => errors.push(e),
        }
    }
    if sets.is_empty() && errors.is_empty() {
        errors.push(format!(
            "{NO_RIG_TAKES} in {} (build them: python -m ac_train.datasets)",
            dir.display()
        ));
    }
    (sets, errors)
}
