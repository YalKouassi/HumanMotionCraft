//! The headless front end: a session over the default rig, and the fixture runner.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod cmdref;
pub mod fixture_spec;
pub mod fixtures_cmd;
pub mod gallery;
pub mod mcp;
pub mod render;
pub mod scaffold;

use std::path::{Path, PathBuf};

use ac_core::Session;
use serde_json::{Value, json};

/// The first `assets` folder at or above `start`.
pub fn find_assets(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|d| d.join("assets"))
        .find(|a| a.join("rig/default_rig.json").is_file())
}

/// The repo an `assets` folder sits in (its parent).
pub fn repo_of(assets: &Path) -> PathBuf {
    match assets.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        Some(_) => PathBuf::from("."),
        None => assets.join(".."),
    }
}

/// `rel` (`/`-separated) under `repo`, with the platform's separators.
pub fn repo_path(repo: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(repo.to_path_buf(), |p, c| p.join(c))
}

/// A session with every command, the default rig, and `library` (installed from the built-ins
/// when it is empty). The same setup as the app ([`ac_io::setup::SessionBuilder`]); a library that
/// can't be installed is an error here, a missing or broken AI model a warning (AI fill then says
/// there is none).
pub fn session(assets: &Path, library: &Path) -> Result<Session, String> {
    Ok(ac_io::setup::SessionBuilder::new(assets)
        .library(library)
        .ai(ac_ai::shared(assets))
        .commands(ac_media::SPECS)
        .commands(ac_textmotion::SPECS)
        .strict()
        .build()?
        .session)
}

/// `actual` matches `expected`: numbers within `tol`, arrays item by item, objects key by key
/// (keys `expected` doesn't name are not checked). Returns where it differs.
///
/// `{"$contains": [items]}` expects an array holding an item matching each of `items`.
pub fn matches(actual: &Value, expected: &Value, tol: f64, at: &str) -> Result<(), String> {
    if let Some(want) = expected.get("$contains").and_then(Value::as_array) {
        let have = actual
            .as_array()
            .ok_or_else(|| format!("{at}: expected an array, got {actual}"))?;
        for (i, w) in want.iter().enumerate() {
            if !have.iter().any(|h| matches(h, w, tol, at).is_ok()) {
                return Err(format!("{at}: no item matches {w} (wanted item {i})"));
            }
        }
        return Ok(());
    }
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (a.as_f64().unwrap_or(f64::NAN), e.as_f64().unwrap_or(f64::NAN));
            if (a - e).abs() <= tol {
                Ok(())
            } else {
                Err(format!("{at}: {a} is not {e} (within {tol})"))
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                return Err(format!("{at}: {} items, expected {}", a.len(), e.len()));
            }
            // a vector (all numbers) is reported whole: one wrong component says little alone
            if a.iter().chain(e).all(Value::is_number) {
                let off = a
                    .iter()
                    .zip(e)
                    .map(|(x, y)| (x.as_f64().unwrap_or(f64::NAN) - y.as_f64().unwrap_or(f64::NAN)).abs())
                    .fold(0.0, f64::max);
                return if off <= tol {
                    Ok(())
                } else {
                    Err(format!(
                        "{at}: {} is not {} (off by {off:.3e}, within {tol})",
                        Value::Array(a.clone()),
                        Value::Array(e.clone())
                    ))
                };
            }
            a.iter()
                .zip(e)
                .enumerate()
                .try_for_each(|(i, (a, e))| matches(a, e, tol, &format!("{at}[{i}]")))
        }
        (Value::Object(a), Value::Object(e)) => e.iter().try_for_each(|(k, e)| match a.get(k) {
            Some(a) => matches(a, e, tol, &format!("{at}.{k}")),
            None => Err(format!("{at}: no {k:?} in {}", Value::Object(a.clone()))),
        }),
        (a, e) if a == e => Ok(()),
        (a, e) => Err(format!("{at}: {a} is not {e}")),
    }
}

/// What a refused command must leave as it was.
fn snapshot(s: &mut Session) -> Value {
    json!({
        "state": s.run("editor.state", &json!({})).unwrap_or(Value::Null),
        "pose": s.run("pose.get", &json!({})).unwrap_or(Value::Null),
    })
}

/// In any text of `v`, `{tmp}` becomes the fixture's scratch folder and `{assets}` the assets
/// folder (for file paths).
fn with_tmp(v: &Value, tmp: &Path, assets: &Path) -> Value {
    let slash = |p: &Path| p.to_string_lossy().replace('\\', "/");
    match v {
        Value::String(s) => Value::String(
            s.replace("{tmp}", &slash(tmp))
                .replace("{assets}", &slash(assets)),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|x| with_tmp(x, tmp, assets)).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, x)| (k.clone(), with_tmp(x, tmp, assets)))
                .collect(),
        ),
        other => other.clone(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub name: String,
    pub feature: String,
    pub failures: Vec<String>,
    /// What each checked step returned: (step index, command, result).
    pub actual: Vec<(usize, String, Value)>,
    /// How long each script step took, milliseconds: (step index, command, ms). For
    /// `ac fixtures --timings` only; never compared or written to a file that is checked.
    pub steps_ms: Vec<(usize, String, f64)>,
}

/// `expected` with every value it names replaced by `actual`'s (keys it does not name stay
/// unnamed; `$contains` lists are left alone). Numbers are rounded to 7 decimals.
pub fn bless(expected: &Value, actual: &Value) -> Value {
    if expected.get("$contains").is_some() {
        return expected.clone();
    }
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => Value::Object(
            e.iter()
                .map(|(k, ev)| (k.clone(), a.get(k).map_or_else(|| ev.clone(), |av| bless(ev, av))))
                .collect(),
        ),
        (Value::Array(e), Value::Array(a)) if e.len() == a.len() => {
            Value::Array(e.iter().zip(a).map(|(x, y)| bless(x, y)).collect())
        }
        (_, Value::Number(n)) => n
            .as_f64()
            .map(|x| (x * 1e7).round() / 1e7)
            .and_then(|x| {
                if x.fract() == 0.0 && x.abs() < 1e15 {
                    Some(json!(x as i64))
                } else {
                    serde_json::Number::from_f64(x).map(Value::Number)
                }
            })
            .unwrap_or_else(|| actual.clone()),
        _ => actual.clone(),
    }
}

/// Run one fixture in a fresh session with a scratch copy of the built-in library.
/// `f` over `items` on as many threads as the machine has (at most 8), results in order. For
/// fixtures, which each run in a library of their own.
pub fn par_map<T: Sync, U: Send>(items: &[T], f: impl Fn(&T) -> U + Sync) -> Vec<U> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(1, 8);
    let mut out: Vec<(usize, U)> = std::thread::scope(|sc| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                sc.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(it) = items.get(i) else { break };
                        mine.push((i, f(it)));
                    }
                    mine
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|w| w.join().unwrap_or_default())
            .collect()
    });
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, u)| u).collect()
}

pub fn run_fixture(assets: &Path, fixture: &Value) -> Outcome {
    let name = fixture["name"].as_str().unwrap_or("(unnamed)").to_string();
    let feature = fixture["feature"].as_str().unwrap_or_default().to_string();
    let mut failures = Vec::new();
    let tol = fixture["expect"]["tolerance"].as_f64().unwrap_or(1e-6);
    // fixtures give the same result everywhere: motion from text comes from the stand-in, never
    // from the optional helper (`ac_textmotion::stand_in`)
    ac_textmotion::stand_in::use_stand_in();
    let lib = std::env::temp_dir().join(format!(
        "{}-fixture-{}-{}",
        ac_core::product::ID,
        std::process::id(),
        ac_io::library::safe(&name).unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&lib);
    let tmp = lib.join("scratch");
    let mut s = match session(assets, &lib) {
        Ok(s) => s,
        Err(e) => {
            return Outcome {
                name,
                feature,
                failures: vec![format!("no session: {e}")],
                ..Outcome::default()
            };
        }
    };
    if let Err(e) = std::fs::create_dir_all(&tmp) {
        return Outcome {
            name,
            feature,
            failures: vec![format!("no scratch folder: {e}")],
            ..Outcome::default()
        };
    }
    // every result is checked against its command's declared outputs, in release builds too
    s.check_outputs = true;
    let mut actual = Vec::new();
    let mut steps_ms = Vec::new();
    for (i, step) in fixture["script"].as_array().into_iter().flatten().enumerate() {
        let cmd = step["command"].as_str().unwrap_or_default();
        let params = with_tmp(&step.get("params").cloned().unwrap_or(json!({})), &tmp, assets);
        let started = std::time::Instant::now();
        let ran = s.run(cmd, &params);
        steps_ms.push((i, cmd.to_string(), started.elapsed().as_secs_f64() * 1e3));
        match ran {
            Ok(v) => {
                if let Some(e) = step.get("expect") {
                    let tol = step["tolerance"].as_f64().unwrap_or(tol);
                    if let Err(m) = matches(&v, e, tol, &format!("step {i} {cmd}")) {
                        failures.push(m);
                    }
                    actual.push((i, cmd.to_string(), v));
                }
            }
            Err(e) => failures.push(format!("step {i} {cmd} was refused: {e}")),
        }
    }
    if let Some(bones) = fixture["expect"]["bones"].as_object() {
        for (bone, e) in bones {
            match s.run("bone.world", &json!({ "bone": bone })) {
                Ok(v) => {
                    if let Err(m) = matches(&v, e, tol, &format!("bone {bone}")) {
                        failures.push(m);
                    }
                }
                Err(err) => failures.push(format!("bone {bone}: {err}")),
            }
        }
    }
    if let Some(state) = fixture["expect"].get("state") {
        let v = s.run("editor.state", &json!({})).unwrap_or(Value::Null);
        if let Err(m) = matches(&v, state, tol, "state") {
            failures.push(m);
        }
    }
    for (i, err) in fixture["errors"].as_array().into_iter().flatten().enumerate() {
        let cmd = err["command"].as_str().unwrap_or_default();
        let before = snapshot(&mut s);
        match s.run(
            cmd,
            &with_tmp(&err.get("params").cloned().unwrap_or(json!({})), &tmp, assets),
        ) {
            Ok(v) => failures.push(format!("error {i} {cmd} was accepted: {v}")),
            Err(e) if e.to_string().trim().is_empty() => {
                failures.push(format!("error {i} {cmd}: empty message"))
            }
            Err(_) => {}
        }
        if snapshot(&mut s) != before {
            failures.push(format!("error {i} {cmd} changed the document"));
        }
    }
    let _ = std::fs::remove_dir_all(&lib);
    Outcome {
        name,
        feature,
        failures,
        actual,
        steps_ms,
    }
}

/// Every `*.json` in `dir` (but `schema.json`), sorted, as (file, parsed). Each must follow the
/// fixture format ([`fixture_spec::check`]): a misspelt key is an error, not a skipped check.
pub fn load_fixtures(dir: &Path) -> Result<Vec<(PathBuf, Value)>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "json") && p.file_name().is_some_and(|n| n != "schema.json")
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|p| {
            let t = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let v = serde_json::from_str(&t).map_err(|e| format!("{}: {e}", p.display()))?;
            fixture_spec::check(&v).map_err(|e| format!("{}: {e}", p.display()))?;
            Ok((p, v))
        })
        .collect()
}

/// Replay a fixture's script (refusals ignored) in a scratch library, then hand the session to
/// `f`.
fn replayed<T>(
    assets: &Path,
    fixture: &Value,
    f: impl FnOnce(&mut Session) -> Result<T, String>,
) -> Result<T, String> {
    let name = fixture["name"].as_str().unwrap_or("fixture");
    let lib = std::env::temp_dir().join(format!(
        "{}-views-{}-{}",
        ac_core::product::ID,
        std::process::id(),
        ac_io::library::safe(name)?
    ));
    let _ = std::fs::remove_dir_all(&lib);
    let tmp = lib.join("scratch");
    let mut s = session(assets, &lib)?;
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    for step in fixture["script"].as_array().into_iter().flatten() {
        let cmd = step["command"].as_str().unwrap_or_default();
        let _ = s.run(
            cmd,
            &with_tmp(&step.get("params").cloned().unwrap_or(json!({})), &tmp, assets),
        );
    }
    let out = f(&mut s);
    let _ = std::fs::remove_dir_all(&lib);
    out
}

fn view_names(fixture: &Value) -> Vec<String> {
    fixture["views"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_str().unwrap_or("front").to_string())
        .collect()
}

/// Replay a fixture's script and render its `views` (a PNG per view) into `dir`, for a person to
/// confirm what the numbers say.
pub fn fixture_views(assets: &Path, fixture: &Value, dir: &Path) -> Result<Vec<PathBuf>, String> {
    let name = fixture["name"].as_str().unwrap_or("fixture").to_string();
    replayed(assets, fixture, |s| {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut pics = render::Pictures::new(assets, &s.document.skeleton.names)?;
        let mut out = Vec::new();
        for view in view_names(fixture) {
            let p = dir.join(format!("{name}-{view}.png"));
            std::fs::write(&p, pics.png(&s.document, &view, 640, 480)?)
                .map_err(|e| format!("{}: {e}", p.display()))?;
            out.push(p);
        }
        Ok(out)
    })
}

/// Golden pictures: size, and how far they may drift. A pixel differs when a channel is more than
/// `GOLDEN_TOLERANCE` apart; a picture fails when more than `GOLDEN_MAX_DIFFER` of its pixels do.
pub const GOLDEN_SIZE: (u32, u32) = (320, 240);
pub const GOLDEN_TOLERANCE: u8 = 24;
pub const GOLDEN_MAX_DIFFER: f64 = 0.002;

/// Check a fixture's views against `goldens/<fixture>-<view>.png`, rendered on the software
/// adapter. `bless`: write them instead. A failure writes the actual picture and a diff (red:
/// pixels that moved) to `diffs`. Returns one message per failed view.
pub fn golden_check(
    assets: &Path,
    fixture: &Value,
    goldens: &Path,
    diffs: &Path,
    bless: bool,
) -> Result<Vec<String>, String> {
    golden_compare(assets, fixture, goldens, diffs, bless, None).map(|(failures, _)| failures)
}

/// One view compared with its golden, for the gallery: the pictures (written into the gallery's
/// folder) and the share of pixels that moved.
#[derive(Debug, Clone)]
pub struct GoldenView {
    pub view: String,
    /// None: there is no golden yet.
    pub golden: Option<PathBuf>,
    pub actual: PathBuf,
    /// None: no golden of the same size to compare with.
    pub diff: Option<PathBuf>,
    pub moved: f64,
}

/// [`golden_check`], and with `keep`, every view's golden, actual and diff pictures written there
/// (passing or not) so the gallery can show them side by side.
pub fn golden_compare(
    assets: &Path,
    fixture: &Value,
    goldens: &Path,
    diffs: &Path,
    bless: bool,
    keep: Option<&Path>,
) -> Result<(Vec<String>, Vec<GoldenView>), String> {
    let name = fixture["name"].as_str().unwrap_or("fixture").to_string();
    let views = view_names(fixture);
    if views.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let (w, h) = GOLDEN_SIZE;
    replayed(assets, fixture, |s| {
        let mut pics = render::Pictures::with_adapter(assets, &s.document.skeleton.names, true)?;
        let mut failures = Vec::new();
        let mut kept = Vec::new();
        for view in views {
            if let Some(dir) = keep.filter(|_| !bless) {
                let k = |what: &str, png: &[u8]| -> Result<PathBuf, String> {
                    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
                    let p = dir.join(format!("{name}-{view}-{what}.png"));
                    std::fs::write(&p, png).map_err(|e| format!("{}: {e}", p.display()))?;
                    Ok(p)
                };
                let actual = pics.rgba(&s.document, &view, w, h, None)?;
                let golden = std::fs::read(goldens.join(format!("{name}-{view}.png"))).ok();
                let decoded = golden
                    .as_deref()
                    .and_then(|b| ac_render::offscreen::unpng(b).ok());
                let (diff, moved) = match &decoded {
                    Some((gw, gh, g)) if (*gw, *gh) == (w, h) => {
                        let (n, pic) = ac_render::offscreen::diff(g, &actual, GOLDEN_TOLERANCE);
                        (
                            Some(k("diff", &ac_render::offscreen::png(w, h, &pic)?)?),
                            n as f64 / f64::from(w * h),
                        )
                    }
                    _ => (None, 1.0),
                };
                kept.push(GoldenView {
                    view: view.clone(),
                    golden: golden.map(|b| k("golden", &b)).transpose()?,
                    actual: k("actual", &ac_render::offscreen::png(w, h, &actual)?)?,
                    diff,
                    moved,
                });
            }
            let file = goldens.join(format!("{name}-{view}.png"));
            let actual = pics.rgba(&s.document, &view, w, h, None)?;
            if bless {
                std::fs::create_dir_all(goldens).map_err(|e| format!("{}: {e}", goldens.display()))?;
                std::fs::write(&file, ac_render::offscreen::png(w, h, &actual)?)
                    .map_err(|e| format!("{}: {e}", file.display()))?;
                continue;
            }
            let save = |what: &str, rgba: &[u8]| -> Result<PathBuf, String> {
                std::fs::create_dir_all(diffs).map_err(|e| format!("{}: {e}", diffs.display()))?;
                let p = diffs.join(format!("{name}-{view}-{what}.png"));
                std::fs::write(&p, ac_render::offscreen::png(w, h, rgba)?)
                    .map_err(|e| format!("{}: {e}", p.display()))?;
                Ok(p)
            };
            let Ok(bytes) = std::fs::read(&file) else {
                let p = save("actual", &actual)?;
                failures.push(format!("{name} {view}: no golden {} (bless with AC_BLESS_VISUAL=1 or --bless-visual after checking {})", file.display(), p.display()));
                continue;
            };
            let (gw, gh, golden) =
                ac_render::offscreen::unpng(&bytes).map_err(|e| format!("{}: {e}", file.display()))?;
            if (gw, gh) != (w, h) {
                let p = save("actual", &actual)?;
                failures.push(format!(
                    "{name} {view}: golden is {gw}x{gh}, pictures are {w}x{h} (actual: {})",
                    p.display()
                ));
                continue;
            }
            let (n, pic) = ac_render::offscreen::diff(&golden, &actual, GOLDEN_TOLERANCE);
            let share = n as f64 / f64::from(w * h);
            if share > GOLDEN_MAX_DIFFER {
                let (a, d) = (save("actual", &actual)?, save("diff", &pic)?);
                failures.push(format!(
                    "{name} {view}: {n} pixels ({:.2}%) moved, more than {:.2}% (actual {}, diff {})",
                    share * 100.0,
                    GOLDEN_MAX_DIFFER * 100.0,
                    a.display(),
                    d.display()
                ));
            }
        }
        Ok((failures, kept))
    })
}

/// `ac fixtures --bless`: write what each checked step returned into its `expect` (only the keys
/// it already names). Returns how many steps changed. The description's hand-worked numbers are
/// not touched: a person checks the blessed values against them.
pub fn bless_file(path: &Path, fixture: &Value, out: &Outcome) -> Result<usize, String> {
    let mut fx = fixture.clone();
    let mut changed = 0;
    if let Some(steps) = fx.get_mut("script").and_then(Value::as_array_mut) {
        for (i, _, v) in &out.actual {
            if let Some(e) = steps.get_mut(*i).and_then(|s| s.get_mut("expect")) {
                let b = bless(e, v);
                if *e != b {
                    *e = b;
                    changed += 1;
                }
            }
        }
    }
    if changed > 0 {
        let text = serde_json::to_string_pretty(&fx).map_err(|e| e.to_string())? + "\n";
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(changed)
}

/// The sample recipes: `<assets>/samples/recipes/<Name>.json`, each `{name, description, script}`
/// (sorted by file name).
pub fn sample_recipes(assets: &Path) -> Result<Vec<Value>, String> {
    let dir = assets.join(ac_io::import::SAMPLES).join("recipes");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|f| f.path())
        .filter(|f| f.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|f| {
            let text = std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", f.display()))
        })
        .collect()
}

/// Make one sample from its recipe: a new animation in a scratch library that has only the
/// built-in poses, the recipe's steps (a few keys, AI fills between them), then saved to the
/// Samples group. Returns the clip file's text: what `<assets>/samples/<name>.clip.json` holds.
pub fn make_sample(assets: &Path, recipe: &Value, scratch: &Path) -> Result<String, String> {
    let name = recipe["name"].as_str().ok_or("a recipe needs a name")?;
    let _ = std::fs::remove_dir_all(scratch);
    // an animations folder (so the built-in clips are not installed) and the built-in poses
    std::fs::create_dir_all(scratch.join("animations")).map_err(|e| e.to_string())?;
    copy_tree(&assets.join("library").join("poses"), &scratch.join("poses"))?;
    let mut s = session(assets, scratch)?;
    let steps = recipe["script"].as_array().ok_or("a recipe needs a script")?;
    let mut run = |id: &str, params: &Value| s.run(id, params).map_err(|e| format!("{name}: {id}: {e}"));
    run(
        "clip.new",
        &json!({ "name": name, "group": ac_io::import::SAMPLES_GROUP }),
    )?;
    for step in steps {
        run(
            step["command"].as_str().unwrap_or_default(),
            &step.get("params").cloned().unwrap_or(json!({})),
        )?;
    }
    run("frame.set", &json!({ "frame": 0 }))?;
    run("clip.save", &json!({}))?;
    let id = format!("{}/{name}", ac_io::import::SAMPLES_GROUP);
    let text = ac_io::library::Library::new(scratch, ac_io::library::Kind::Animations).read(&id);
    let _ = std::fs::remove_dir_all(scratch);
    text
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    for e in std::fs::read_dir(from)
        .map_err(|e| format!("{}: {e}", from.display()))?
        .flatten()
    {
        let dst = to.join(e.file_name());
        if e.path().is_dir() {
            copy_tree(&e.path(), &dst)?;
        } else {
            std::fs::copy(e.path(), &dst).map_err(|err| format!("{}: {err}", dst.display()))?;
        }
    }
    Ok(())
}
