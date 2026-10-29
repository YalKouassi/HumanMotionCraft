//! An animation as pictures: the frames of a motion from one camera, an animated GIF of them, and
//! an MP4 through ffmpeg. The one encoder of the workspace: MCP's `animation_preview` (headless
//! and in the app), `clip.video` and `ac video` (through `ac_media::Encoder`) all write with it.
//!
//! The GIF encoder is our own (no dependency): one shared palette of the commonest colours over
//! all frames, LZW-coded. Deterministic: the same frames give the same bytes. Frames go in one at
//! a time ([`GifWriter`], [`Mp4Writer`]); a GIF keeps each frame's colour cells (two bytes a
//! pixel) until it is finished, because its palette is chosen from all of them.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

use crate::offscreen::{self, Framing, MAX_SIDE, Offscreen, Posed};

/// Most frames in one animation preview.
pub const MAX_FRAMES: usize = 600;
/// Most pixels (all frames together) one GIF holds while it is written: 320 MB of colour cells.
/// A longer or larger animation is an MP4.
pub const MAX_GIF_PIXELS: usize = 160_000_000;
/// Colour cells: 5 bits a channel.
const CELLS: usize = 1 << 15;

impl Offscreen {
    /// RGBA rows of each posed frame from a named view. `fixed`: one camera that sees every frame
    /// (travel and height show); else the camera follows the root, at the view's usual distance.
    pub fn frames(
        &mut self,
        view_name: &str,
        (w, h): (u32, u32),
        frames: &[Posed],
        rest: &[[f64; 3]],
        fixed: bool,
    ) -> Result<Vec<Vec<u8>>, String> {
        if frames.is_empty() {
            return Err("no frames to show".into());
        }
        if frames.len() > MAX_FRAMES {
            return Err(format!("{} frames; at most {MAX_FRAMES}", frames.len()));
        }
        let aspect = f64::from(w) / f64::from(h.max(1));
        let all = if fixed {
            let points: Vec<[f64; 3]> = frames.iter().flat_map(|f| f.1.iter().copied()).collect();
            Some(Framing::tight(&points, view_name, aspect)?)
        } else {
            None
        };
        let mut out = Vec::with_capacity(frames.len());
        for (rots, pos) in frames {
            let camera = match all {
                Some(c) => c,
                None => {
                    let root = pos.first().copied().unwrap_or_default();
                    offscreen::view(view_name, glam::Vec3::new(root[0] as f32, root[1] as f32, 0.9))?
                }
            };
            out.push(self.render(w, h, &camera, (rots, pos), rest)?);
        }
        Ok(out)
    }
}

/// Centiseconds a frame shows for at `fps` (GIF's unit; at least 2, which every viewer honours).
pub fn delay_cs(fps: f64) -> u16 {
    if !(fps.is_finite() && fps > 0.0) {
        return 4;
    }
    (100.0 / fps).round().clamp(2.0, 6000.0) as u16
}

/// An animated GIF (looping) of RGBA frames `w` x `h`, each shown `delay` centiseconds.
pub fn gif(w: u32, h: u32, frames: &[Vec<u8>], delay: u16) -> Result<Vec<u8>, String> {
    let mut g = GifWriter::new(w, h, delay)?;
    for f in frames {
        g.push(f)?;
    }
    g.finish()
}

/// An animated GIF written a frame at a time: [`GifWriter::push`] each RGBA frame, then
/// [`GifWriter::finish`] for the file's bytes.
pub struct GifWriter {
    size: (u32, u32),
    delay: u16,
    /// Pixels a colour cell holds, and the sums of their channels, over every frame so far.
    count: Vec<u64>,
    sum: Vec<[u64; 3]>,
    /// Every frame's pixels as colour cells, one after another.
    cells: Vec<u16>,
    frames: usize,
}

impl GifWriter {
    /// A GIF of `w` x `h` frames, each shown `delay` centiseconds.
    pub fn new(w: u32, h: u32, delay: u16) -> Result<GifWriter, String> {
        if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
            return Err(format!("GIF size {w}x{h} must be 1..={MAX_SIDE} on each side"));
        }
        Ok(GifWriter {
            size: (w, h),
            delay,
            count: vec![0; CELLS],
            sum: vec![[0; 3]; CELLS],
            cells: Vec::new(),
            frames: 0,
        })
    }

    /// Frames so far.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Add a frame: RGBA rows of the writer's size.
    pub fn push(&mut self, rgba: &[u8]) -> Result<(), String> {
        let (w, h) = self.size;
        let px = (w as usize) * (h as usize);
        if rgba.len() != px * 4 {
            return Err(format!("frame {} is not {w}x{h} RGBA", self.frames));
        }
        if self.cells.len() + px > MAX_GIF_PIXELS {
            return Err(format!(
                "a GIF of {} frames at {w}x{h} is more than {MAX_GIF_PIXELS} pixels: make an MP4, or fewer or smaller frames",
                self.frames + 1
            ));
        }
        self.cells.reserve(px);
        for c in rgba.as_chunks::<4>().0 {
            let k = cell(c);
            if let (Some(n), Some(s)) = (self.count.get_mut(k), self.sum.get_mut(k)) {
                *n += 1;
                for (acc, v) in s.iter_mut().zip(c) {
                    *acc += u64::from(*v);
                }
            }
            self.cells.push(k as u16);
        }
        self.frames += 1;
        Ok(())
    }

    /// The GIF's bytes.
    pub fn finish(self) -> Result<Vec<u8>, String> {
        if self.frames == 0 {
            return Err("a GIF needs at least one frame".into());
        }
        let (w, h) = self.size;
        let px = (w as usize) * (h as usize);
        let palette = Palette::of(&self.count, &self.sum);
        let mut out = Vec::with_capacity(self.cells.len() / 4 + 1024);
        out.extend_from_slice(b"GIF89a");
        out.extend_from_slice(&(w as u16).to_le_bytes());
        out.extend_from_slice(&(h as u16).to_le_bytes());
        // a global colour table of 256 entries, 8 bits a channel
        out.extend_from_slice(&[0xF7, 0, 0]);
        for c in &palette.colours {
            out.extend_from_slice(c);
        }
        // loop forever (the NETSCAPE2.0 application extension)
        out.extend_from_slice(&[0x21, 0xFF, 0x0B]);
        out.extend_from_slice(b"NETSCAPE2.0");
        out.extend_from_slice(&[0x03, 0x01, 0x00, 0x00, 0x00]);
        let mut indices = vec![0u8; px];
        for f in self.cells.chunks(px.max(1)) {
            for (dst, k) in indices.iter_mut().zip(f) {
                *dst = palette.map.get(usize::from(*k)).copied().unwrap_or(0);
            }
            // graphic control: no transparency, the delay
            out.extend_from_slice(&[0x21, 0xF9, 0x04, 0x00]);
            out.extend_from_slice(&self.delay.to_le_bytes());
            out.extend_from_slice(&[0x00, 0x00]);
            // the image: the whole screen, no local colour table
            out.push(0x2C);
            out.extend_from_slice(&[0, 0, 0, 0]);
            out.extend_from_slice(&(w as u16).to_le_bytes());
            out.extend_from_slice(&(h as u16).to_le_bytes());
            out.push(0);
            out.push(8); // LZW minimum code size
            for block in lzw(&indices).chunks(255) {
                out.push(block.len() as u8);
                out.extend_from_slice(block);
            }
            out.push(0);
        }
        out.push(0x3B);
        Ok(out)
    }
}

/// 256 colours for all frames: the commonest 5-bit-a-channel colour cells (each the mean of its
/// pixels), every other cell mapped to its nearest entry.
struct Palette {
    colours: Vec<[u8; 3]>,
    /// Cell (15 bits) -> palette index.
    map: Vec<u8>,
}

fn cell(c: &[u8]) -> usize {
    let ch = |i: usize| usize::from(c.get(i).copied().unwrap_or(0) >> 3);
    (ch(0) << 10) | (ch(1) << 5) | ch(2)
}

impl Palette {
    /// From the pixels each colour cell holds and the sums of their channels.
    fn of(count: &[u64], sum: &[[u64; 3]]) -> Palette {
        let mut used: Vec<usize> = (0..count.len())
            .filter(|&k| count.get(k).is_some_and(|n| *n > 0))
            .collect();
        // commonest first; ties by cell, so the palette never depends on anything but the pixels
        used.sort_by_key(|&k| (std::cmp::Reverse(count.get(k).copied().unwrap_or(0)), k));
        let mut colours: Vec<[u8; 3]> = used
            .iter()
            .take(256)
            .map(|&k| {
                let n = count.get(k).copied().unwrap_or(1).max(1);
                let s = sum.get(k).copied().unwrap_or_default();
                [(s[0] / n) as u8, (s[1] / n) as u8, (s[2] / n) as u8]
            })
            .collect();
        let mut map = vec![0u8; CELLS];
        for &k in &used {
            let centre = [
                (((k >> 10) as u8) << 3) | 4,
                ((((k >> 5) & 31) as u8) << 3) | 4,
                (((k & 31) as u8) << 3) | 4,
            ];
            if let Some(m) = map.get_mut(k) {
                *m = nearest(&colours, centre);
            }
        }
        colours.resize(256, [0, 0, 0]);
        Palette { colours, map }
    }

    #[cfg(test)]
    fn index(&self, c: &[u8]) -> u8 {
        self.map.get(cell(c)).copied().unwrap_or(0)
    }
}

fn nearest(colours: &[[u8; 3]], c: [u8; 3]) -> u8 {
    let d = |p: &[u8; 3]| {
        p.iter()
            .zip(c)
            .map(|(a, b)| (i32::from(*a) - i32::from(b)).pow(2))
            .sum::<i32>()
    };
    colours
        .iter()
        .enumerate()
        .min_by_key(|(i, p)| (d(p), *i))
        .map_or(0, |(i, _)| i as u8)
}

/// GIF's variable-width LZW of 8-bit indices (codes least significant bit first).
fn lzw(indices: &[u8]) -> Vec<u8> {
    const CLEAR: u16 = 256;
    const END: u16 = 257;
    let mut bits = Bits::default();
    let mut dict: HashMap<(u16, u8), u16> = HashMap::new();
    let mut size = 9u32;
    let mut next = 258u16;
    bits.put(CLEAR, size);
    let mut prefix: Option<u16> = None;
    for &px in indices {
        let Some(p) = prefix else {
            prefix = Some(u16::from(px));
            continue;
        };
        if let Some(&code) = dict.get(&(p, px)) {
            prefix = Some(code);
            continue;
        }
        bits.put(p, size);
        if next < 4096 {
            dict.insert((p, px), next);
            next += 1;
            if u32::from(next) > (1 << size) && size < 12 {
                size += 1;
            }
        } else {
            bits.put(CLEAR, size);
            dict.clear();
            size = 9;
            next = 258;
        }
        prefix = Some(u16::from(px));
    }
    if let Some(p) = prefix {
        bits.put(p, size);
    }
    bits.put(END, size);
    bits.finish()
}

#[derive(Default)]
struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, code: u16, size: u32) {
        self.acc |= u32::from(code) << self.n;
        self.n += size;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc as u8);
        }
        self.out
    }
}

/// ffmpeg: `$AC_FFMPEG`, else `ffmpeg` on the PATH, else the workspace's usual install.
pub fn ffmpeg() -> PathBuf {
    if let Some(p) = std::env::var_os("AC_FFMPEG") {
        return p.into();
    }
    let usual = PathBuf::from("C:/tools/ffmpeg/bin/ffmpeg.exe");
    let on_path = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !on_path && usual.is_file() {
        usual
    } else {
        "ffmpeg".into()
    }
}

/// Write RGBA frames `w` x `h` (even sizes) to an H.264 MP4 at `out`, played at `fps`.
pub fn mp4(out: &Path, w: u32, h: u32, fps: f64, frames: &[Vec<u8>]) -> Result<(), String> {
    let mut m = Mp4Writer::create(out, w, h, fps)?;
    for f in frames {
        m.push(f)?;
    }
    m.finish().map(|_| ())
}

/// An H.264 MP4 written a frame at a time through ffmpeg (which must be installed): frames go
/// straight to it, nothing is kept.
pub struct Mp4Writer {
    /// `None` once finished.
    child: Option<(Child, ChildStdin)>,
    size: (u32, u32),
    frames: usize,
}

impl Mp4Writer {
    /// Start an MP4 of `w` x `h` frames (even sizes, as x264 wants) at `fps` into `out`.
    pub fn create(out: &Path, w: u32, h: u32, fps: f64) -> Result<Mp4Writer, String> {
        if !w.is_multiple_of(2) || !h.is_multiple_of(2) || w == 0 || h == 0 {
            return Err(format!("an MP4 needs even sizes, got {w}x{h}"));
        }
        if !(fps.is_finite() && fps > 0.0 && fps <= 240.0) {
            return Err(format!("fps {fps} must be above 0 and at most 240"));
        }
        let mut child = Command::new(ffmpeg())
            .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s"])
            .arg(format!("{w}x{h}"))
            .arg("-r")
            .arg(fps.to_string())
            .args(["-i", "-", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18"])
            .arg(out)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                format!(
                    "MP4 needs ffmpeg, which could not be started ({e}): install it, or set AC_FFMPEG to its path; GIF needs nothing"
                )
            })?;
        let stdin = child.stdin.take().ok_or("ffmpeg has no input")?;
        Ok(Mp4Writer {
            child: Some((child, stdin)),
            size: (w, h),
            frames: 0,
        })
    }

    /// Add a frame: RGBA rows of the writer's size.
    pub fn push(&mut self, rgba: &[u8]) -> Result<(), String> {
        let (w, h) = self.size;
        if rgba.len() != (w as usize) * (h as usize) * 4 {
            return Err(format!("a frame of {} bytes is not {w}x{h} RGBA", rgba.len()));
        }
        let Some((_, stdin)) = self.child.as_mut() else {
            return Err("the video is already finished".into());
        };
        stdin
            .write_all(rgba)
            .map_err(|e| format!("ffmpeg stopped taking frames: {e}"))?;
        self.frames += 1;
        Ok(())
    }

    /// Close the file once ffmpeg is done; returns how many frames it holds.
    pub fn finish(mut self) -> Result<usize, String> {
        let Some((child, stdin)) = self.child.take() else {
            return Err("the video is already finished".into());
        };
        drop(stdin);
        let done = child.wait_with_output().map_err(|e| format!("ffmpeg: {e}"))?;
        if !done.status.success() {
            return Err(format!(
                "ffmpeg failed ({}): {}",
                done.status,
                String::from_utf8_lossy(&done.stderr).trim()
            ));
        }
        if self.frames == 0 {
            return Err("no frames to write".into());
        }
        Ok(self.frames)
    }
}

impl Drop for Mp4Writer {
    /// A video given up on (a frame failed to render): ffmpeg is stopped rather than left
    /// waiting for frames. Best effort: it may have ended already.
    fn drop(&mut self) {
        if let Some((child, _)) = &mut self.child
            && child.kill().is_ok()
        {
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

    use super::*;

    /// GIF's LZW back to indices (the decoder side, for the round trip).
    fn unlzw(data: &[u8]) -> Vec<u8> {
        let mut table: Vec<Vec<u8>> = (0..=255u8).map(|i| vec![i]).collect();
        table.push(Vec::new());
        table.push(Vec::new());
        let (mut size, mut acc, mut n, mut pos) = (9u32, 0u32, 0u32, 0usize);
        let mut prev: Option<Vec<u8>> = None;
        let mut out = Vec::new();
        loop {
            while n < size {
                acc |= u32::from(data[pos]) << n;
                pos += 1;
                n += 8;
            }
            let code = (acc & ((1 << size) - 1)) as usize;
            acc >>= size;
            n -= size;
            if code == 256 {
                table.truncate(258);
                size = 9;
                prev = None;
                continue;
            }
            if code == 257 {
                return out;
            }
            let entry = match (table.get(code), &prev) {
                (Some(e), _) => e.clone(),
                (None, Some(p)) => {
                    let mut e = p.clone();
                    e.push(p[0]);
                    e
                }
                (None, None) => panic!("bad first code"),
            };
            if let Some(p) = prev {
                let mut e = p.clone();
                e.push(entry[0]);
                table.push(e);
                if table.len() == (1 << size) && size < 12 {
                    size += 1;
                }
            }
            out.extend_from_slice(&entry);
            prev = Some(entry);
        }
    }

    #[test]
    fn lzw_round_trips_long_and_varied_runs() {
        // enough distinct strings to fill the 4096-code table and clear it several times
        let data: Vec<u8> = (0..200_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8 ^ (i / 7) as u8)
            .collect();
        assert_eq!(unlzw(&lzw(&data)), data);
        let flat = vec![7u8; 100_000];
        assert_eq!(unlzw(&lzw(&flat)), flat);
        assert_eq!(unlzw(&lzw(&[1])), vec![1]);
    }

    #[test]
    fn gif_has_the_frames_and_is_deterministic() {
        let (w, h) = (6u32, 4u32);
        let frame = |shade: u8| -> Vec<u8> {
            (0..w * h)
                .flat_map(|i| [shade, (i * 10) as u8, 200, 255])
                .collect()
        };
        let frames = vec![frame(0), frame(120), frame(250)];
        let a = gif(w, h, &frames, delay_cs(30.0)).unwrap();
        assert_eq!(&a[..6], b"GIF89a");
        assert_eq!(a.last(), Some(&0x3B));
        // three image descriptors, each after a graphic control block with a 3 cs delay
        let gce = a
            .windows(6)
            .filter(|x| *x == [0x21, 0xF9, 0x04, 0x00, 3, 0])
            .count();
        assert_eq!(gce, 3);
        assert_eq!(a, gif(w, h, &frames, 3).unwrap());
        assert!(gif(w, h, &[vec![0; 4]], 3).is_err());
        assert!(gif(0, h, &frames, 3).is_err());
        assert!(gif(w, h, &[], 3).is_err());
        // a frame at a time gives the same bytes
        let mut g = GifWriter::new(w, h, 3).unwrap();
        for f in &frames {
            g.push(f).unwrap();
        }
        assert_eq!(g.frames(), 3);
        assert_eq!(g.finish().unwrap(), a);
    }

    #[test]
    fn the_palette_keeps_exact_colours_when_there_are_few() {
        let mut g = GifWriter::new(4, 4, 3).unwrap();
        g.push(&[10u8, 20, 30, 255, 200, 100, 50, 255].repeat(8)).unwrap();
        let p = Palette::of(&g.count, &g.sum);
        assert_eq!(p.colours[p.index(&[10, 20, 30, 255]) as usize], [10, 20, 30]);
        assert_eq!(p.colours[p.index(&[200, 100, 50, 255]) as usize], [200, 100, 50]);
    }

    #[test]
    fn delays_are_sane() {
        assert_eq!(delay_cs(30.0), 3);
        assert_eq!(delay_cs(24.0), 4);
        assert_eq!(delay_cs(1000.0), 2);
        assert_eq!(delay_cs(f64::NAN), 4);
    }
}
