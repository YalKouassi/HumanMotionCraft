//! The community library: clips other people shared, to browse, play, install and rate, and your
//! own to share. `community.browse` lists them, `community.fetch` unpacks one to play,
//! `community.install` puts one in the library (group `Community`, its credit kept in
//! [`record`]), `community.rate`, `community.report`, `community.share`, `community.mine` and
//! `community.remove` do what they say, `community.verify` hands in the captcha and
//! `community.account` says where the account stands.
//!
//! The server is `services/community/`; [`backend`] is the seam ([`http`] the live server,
//! [`fake`] the fixtures'). Every command may wait on the network, so the window runs it off its
//! UI thread first ([`prepare`]) and then runs the command, which finds its result ready. None is
//! undoable: they change the library or the server, never the document.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod backend;
pub mod fake;
pub mod http;
pub mod record;
mod returns;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use ac_core::Document;
use ac_core::clipfile;
use ac_core::command::{self as cmd, CommandSpec, Kind as K, ParamSpec};
use ac_io::library::{Kind, Library};
use serde_json::{Map, Value, json};

use backend::{CAPTCHA_NEEDED, Listing, Query, Sort, Upload};
use record::{CACHE, GROUP, Install, ORIGIN, Record};

type P = Map<String, Value>;
type Out = Result<Value, String>;

pub const LICENCES: &[&str] = &["CC0-1.0", "CC-BY-4.0"];
pub const MAX_TITLE: usize = 80;
pub const MAX_DESCRIPTION: usize = 2000;
pub const MAX_CREDIT: usize = 200;
pub const MAX_NAME: usize = 40;
pub const MAX_REASON: usize = 500;
pub const MAX_TAGS: usize = 10;
pub const MAX_TAG: usize = 24;
/// The largest clip shared, gzipped, and preview (bytes), as the server takes them.
pub const MAX_CLIP_GZ: usize = 10 * 1024 * 1024;
pub const MAX_PREVIEW: usize = 1024 * 1024;
/// The largest clip read, unzipped.
const MAX_UNZIPPED: u64 = 100 * 1024 * 1024;

const ID: ParamSpec = ParamSpec::new("id", K::Text, "the community clip's id").required();

pub static SPECS: &[CommandSpec] = &[
    CommandSpec::new("community.browse", "Browse the community", community_browse)
        .icon("globe")
        .inputs(&[
            ParamSpec::new(
                "search",
                K::Text,
                "words that must all be in the title, description or tags",
            ),
            ParamSpec::new("tags", K::TextList, "tags the clip must all have"),
            ParamSpec::new(
                "sort",
                K::Choice(&["top", "new", "downloads"]),
                "top (best rated, the default), new, or most downloaded",
            ),
            ParamSpec::new("page", K::Integer, "which page, from 0").range(0.0, 10000.0),
            ParamSpec::new("limit", K::Integer, "clips a page (default 50)").range(1.0, 100.0),
        ])
        .outputs(returns::BROWSE),
    CommandSpec::new("community.fetch", "Fetch community clip", community_fetch)
        .icon("download")
        .writes()
        .inputs(&[ID])
        .outputs(returns::FETCH),
    CommandSpec::new("community.install", "Install community clip", community_install)
        .icon("download")
        .writes()
        .inputs(&[ID])
        .outputs(returns::INSTALL),
    CommandSpec::new("community.rate", "Rate community clip", community_rate)
        .icon("star")
        .writes()
        .inputs(&[
            ID,
            ParamSpec::new("stars", K::Integer, "1 to 5")
                .range(1.0, 5.0)
                .required(),
        ])
        .outputs(returns::RATE),
    CommandSpec::new("community.report", "Report community clip", community_report)
        .icon("flag")
        .writes()
        .inputs(&[
            ID,
            ParamSpec::new("reason", K::Text, "what's wrong with it (500 characters at most)"),
        ])
        .outputs(returns::REPORT),
    CommandSpec::new("community.verify", "Pass the community captcha", community_verify)
        .icon("check")
        .writes()
        .inputs(&[ParamSpec::new("token", K::Text, "the token the captcha page gave").required()])
        .outputs(returns::VERIFY),
    CommandSpec::new("community.share", "Share to community", community_share)
        .icon("upload")
        .writes()
        .inputs(&[
            ParamSpec::new("id", K::Text, "one of your animations in the library").required(),
            ParamSpec::new(
                "title",
                K::Text,
                "80 characters at most (default: the animation's name)",
            ),
            ParamSpec::new("description", K::Text, "2000 characters at most"),
            ParamSpec::new(
                "tags",
                K::TextList,
                "10 at most: letters, digits and hyphens, 24 each",
            ),
            ParamSpec::new(
                "licence",
                K::Choice(LICENCES),
                "CC0-1.0 (anyone, any use) or CC-BY-4.0 (with credit)",
            )
            .required(),
            ParamSpec::new(
                "credit",
                K::Text,
                "who or what to credit (200 characters at most)",
            ),
            ParamSpec::new("name", K::Text, "the name shown as the author (40 at most)"),
            ParamSpec::new(
                "preview",
                K::Text,
                "its frames side by side as one PNG strip (1 MB at most), as the Library draws thumbnails",
            ),
        ])
        .outputs(returns::SHARE),
    CommandSpec::new("community.mine", "My community uploads", community_mine)
        .icon("user")
        .outputs(returns::MINE),
    CommandSpec::new("community.remove", "Remove my community clip", community_remove)
        .icon("trash")
        .writes()
        .inputs(&[ID])
        .outputs(returns::REMOVE),
    CommandSpec::new("community.account", "Community account", community_account)
        .icon("user")
        .outputs(returns::ACCOUNT),
];

/// What a command needs from the document: where the library and the shipped built-ins are.
#[derive(Clone, Debug)]
pub struct Ctx {
    library: Option<PathBuf>,
    assets: Option<PathBuf>,
}

impl Ctx {
    pub fn of(doc: &Document) -> Ctx {
        Ctx {
            library: doc.library.clone(),
            assets: doc.assets.clone(),
        }
    }

    fn root(&self) -> Result<&Path, String> {
        self.library
            .as_deref()
            .ok_or_else(|| "no library folder is set (start the app with one)".to_string())
    }
}

/// Results made off the UI thread ([`prepare`]), by command and parameters.
static READY: Mutex<Vec<(String, Out)>> = Mutex::new(Vec::new());

fn key(command: &str, p: &P) -> String {
    format!("{command} {}", Value::Object(p.clone()))
}

/// Runs `command` with `p` without the document: a front end calls this off its UI thread, then
/// runs the command, which returns this result at once.
pub fn prepare(ctx: &Ctx, command: &str, p: &P) {
    let out = run(ctx, command, p);
    READY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((key(command, p), out));
}

/// `command` with `p` failed off the UI thread before it had a result: it returns `why`.
pub fn give_up(command: &str, p: &P, why: &str) {
    READY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((key(command, p), Err(why.to_string())));
}

/// Whether `command` with `p` has a result made off the UI thread waiting.
pub fn is_ready(command: &str, p: &P) -> bool {
    let k = key(command, p);
    READY
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .any(|(r, _)| *r == k)
}

/// Whether the result waiting for `command` with `p` is the captcha refusal; if so it is dropped
/// (the window passes the captcha and runs the command again).
pub fn take_captcha(command: &str, p: &P) -> bool {
    let k = key(command, p);
    let mut ready = READY.lock().unwrap_or_else(PoisonError::into_inner);
    let at = ready
        .iter()
        .position(|(r, out)| *r == k && out.as_ref().is_err_and(|e| e.starts_with(CAPTCHA_NEEDED)));
    at.map(|i| ready.remove(i)).is_some()
}

/// Community clip `id`'s file text, downloaded and checked (for a picture of it; nothing is
/// written).
pub fn clip_text(id: &str) -> Result<String, String> {
    download(id).map(|(_, text)| text)
}

/// Whether `command` is one of these (every one may wait on the network).
pub fn waits(command: &str) -> bool {
    command.starts_with("community.")
}

fn finish(doc: &Document, command: &str, p: &P) -> Out {
    let k = key(command, p);
    let mut ready = READY.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(i) = ready.iter().position(|(r, _)| *r == k) {
        return ready.remove(i).1;
    }
    drop(ready);
    run(&Ctx::of(doc), command, p)
}

fn community_browse(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.browse", p)
}
fn community_fetch(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.fetch", p)
}
fn community_install(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.install", p)
}
fn community_rate(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.rate", p)
}
fn community_report(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.report", p)
}
fn community_verify(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.verify", p)
}
fn community_share(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.share", p)
}
fn community_mine(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.mine", p)
}
fn community_remove(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.remove", p)
}
fn community_account(doc: &mut Document, p: &P) -> Out {
    finish(doc, "community.account", p)
}

/// One command, start to end.
pub fn run(ctx: &Ctx, command: &str, p: &P) -> Out {
    run_inner(ctx, command, p).map_err(|e| captcha_hint(&e))
}

fn run_inner(ctx: &Ctx, command: &str, p: &P) -> Out {
    let id = || cmd::text(p, "id").unwrap_or_default().trim().to_string();
    match command {
        "community.browse" => browse(ctx, p),
        "community.fetch" => fetch(ctx, &id()),
        "community.install" => install(ctx, &id()),
        "community.rate" => {
            let stars = p.get("stars").and_then(Value::as_i64).unwrap_or(0);
            let stars = u8::try_from(stars).ok().filter(|s| (1..=5).contains(s));
            let stars = stars.ok_or("stars must be 1 to 5")?;
            let id = id();
            let (rating, ratings) = backend::with(|b| b.rate(&id, stars))?;
            Ok(json!({ "id": id, "rating": rating, "ratings": ratings }))
        }
        "community.report" => {
            let reason = cmd::text(p, "reason").unwrap_or_default().trim();
            if reason.chars().count() > MAX_REASON {
                return Err(format!(
                    "the reason is too long ({MAX_REASON} characters at most)"
                ));
            }
            let id = id();
            backend::with(|b| b.report(&id, reason))?;
            Ok(json!({ "id": id, "reported": true }))
        }
        "community.verify" => {
            let token = cmd::text(p, "token").unwrap_or_default().trim();
            if token.is_empty() {
                return Err("token is empty".into());
            }
            backend::with(|b| b.verify(token))?;
            Ok(json!({ "verified": true }))
        }
        "community.share" => share(ctx, p),
        "community.mine" => {
            let mine = backend::with(|b| b.mine())?;
            Ok(json!({ "clips": mine }))
        }
        "community.remove" => {
            let id = id();
            backend::with(|b| b.remove(&id))?;
            Ok(json!({ "id": id, "status": "removed" }))
        }
        "community.account" => {
            let a = backend::with(|b| b.account())?;
            let page = backend::with(|b| b.captcha_page()).unwrap_or_default();
            Ok(json!({ "signed_in": a.signed_in, "verified": a.verified, "name": a.name, "captcha": page }))
        }
        other => Err(format!("{other} is not a community command")),
    }
}

/// The captcha refusal, with where to pass it.
fn captcha_hint(e: &str) -> String {
    if !e.starts_with(CAPTCHA_NEEDED) {
        return e.to_string();
    }
    let page = backend::with(|b| b.captcha_page()).unwrap_or_default();
    format!(
        "{CAPTCHA_NEEDED}: sharing and reporting need one captcha (the window opens {page}); \
         then run community.verify with the token it gives"
    )
}

fn browse(ctx: &Ctx, p: &P) -> Out {
    let sort = match cmd::text(p, "sort").unwrap_or("top") {
        "new" => Sort::New,
        "downloads" => Sort::Downloads,
        _ => Sort::Top,
    };
    let int = |name: &str, default: i64| p.get(name).and_then(Value::as_i64).unwrap_or(default);
    let q = Query {
        search: backend::words([cmd::text(p, "search").unwrap_or_default()]),
        tags: backend::words(cmd::texts(p, "tags").iter().map(String::as_str)),
        sort,
        page: u32::try_from(int("page", 0)).unwrap_or(0),
        limit: u32::try_from(int("limit", 50).clamp(1, 100)).unwrap_or(50),
    };
    let found = backend::with(|b| b.browse(&q))?;
    let (rec, lib) = match ctx.root() {
        Ok(root) => (
            Record::load(root).unwrap_or_default(),
            Some(Library::new(root, Kind::Animations)),
        ),
        Err(_) => (Record::default(), None),
    };
    let clips: Vec<Value> = found
        .iter()
        .map(|l| {
            let installed = lib
                .as_ref()
                .and_then(|lib| rec.installed(&l.id, |id| lib.exists(id)))
                .unwrap_or_default();
            listing_json(l, &installed)
        })
        .collect();
    Ok(json!({ "count": clips.len(), "clips": clips }))
}

fn listing_json(l: &Listing, installed: &str) -> Value {
    let seconds = if l.fps > 0.0 {
        ((l.frames as f64 / l.fps) * 100.0).round() / 100.0
    } else {
        0.0
    };
    json!({
        "id": l.id, "title": l.title, "description": l.description, "author": l.author,
        "tags": l.tags, "licence": l.licence, "credit": l.credit, "fps": l.fps, "frames": l.frames,
        "seconds": seconds, "rating": l.rating, "ratings": l.ratings, "downloads": l.downloads,
        "published": l.published, "preview": l.preview, "installed": installed,
    })
}

/// The clip `id`, downloaded, its hash checked and unpacked: its listing and file text.
fn download(id: &str) -> Result<(Listing, String), String> {
    let l = backend::with(|b| b.listing(id))?;
    let gz = backend::with(|b| b.file(&l))?;
    if fake::sha256(&gz) != l.sha256 {
        return Err(format!(
            "{id:?}'s file doesn't match its hash (damaged on the way?); nothing installed"
        ));
    }
    let mut text = String::new();
    flate2::read::GzDecoder::new(gz.as_slice())
        .take(MAX_UNZIPPED)
        .read_to_string(&mut text)
        .map_err(|e| format!("{id:?}'s file isn't a gzipped clip: {e}"))?;
    clipfile::load_clip(&text).map_err(|e| format!("{id:?} isn't a clip this app can open: {e}"))?;
    Ok((l, text))
}

fn fetch(ctx: &Ctx, id: &str) -> Out {
    let (l, text) = download(id)?;
    let dir = ctx.root()?.join(CACHE);
    std::fs::create_dir_all(&dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    let name = ac_io::library::safe(id)?;
    let path = dir.join(format!("{name}.clip.json"));
    craft_atomic::atomic_write(&path, text.as_bytes())
        .map_err(|e| format!("can't write {}: {e}", path.display()))?;
    Ok(json!({ "id": id, "path": path.to_string_lossy(), "clip": listing_json(&l, "") }))
}

fn install(ctx: &Ctx, id: &str) -> Out {
    let root = ctx.root()?;
    let lib = Library::new(root, Kind::Animations);
    let mut rec = Record::load(root)?;
    if let Some(have) = rec.installed(id, |i| lib.exists(i)) {
        return Ok(json!({ "id": have, "clip": id, "already": true, "notes": [] }));
    }
    let (l, text) = download(id)?;
    let mut clip = clipfile::load_clip(&text)?;
    let into = lib.free_id(GROUP, &l.title)?;
    clip.name = into.rsplit('/').next().unwrap_or(&l.title).to_string();
    clip.group = GROUP.into();
    clip.origin = format!("{ORIGIN}{id}");
    lib.write(&into, &clipfile::save_clip(&clip))?;
    rec.installs.insert(
        into.clone(),
        Install {
            clip: id.into(),
            title: l.title.clone(),
            author: l.author.clone(),
            licence: l.licence.clone(),
            credit: l.credit.clone(),
        },
    );
    rec.save(root)?;
    let mut notes = Vec::new();
    if let Err(e) = backend::with(|b| b.counted(id)) {
        notes.push(format!("installed, but the download wasn't counted: {e}"));
    }
    Ok(json!({ "id": into, "clip": id, "already": false, "notes": notes }))
}

fn limited(p: &P, name: &str, max: usize) -> Result<String, String> {
    let s = cmd::text(p, name).unwrap_or_default().trim().to_string();
    if s.chars().count() > max {
        return Err(format!("{name} is too long ({max} characters at most)"));
    }
    Ok(s)
}

fn share(ctx: &Ctx, p: &P) -> Out {
    let id = cmd::text(p, "id").unwrap_or_default().trim().to_string();
    let lib = Library::new(ctx.root()?, Kind::Animations);
    if !lib.exists(&id) {
        return Err(format!(
            "no animation {id:?} in the library (poses can't be shared)"
        ));
    }
    if let Some(a) = &ctx.assets
        && Library::new(&a.join("library"), Kind::Animations).exists(&id)
    {
        return Err(format!("{id} is a built-in: share one of your own animations"));
    }
    let mut clip = clipfile::load_clip(&lib.read(&id)?)?;
    if clip.origin.starts_with(ORIGIN) {
        return Err(format!(
            "{id} came from the community: share one of your own animations"
        ));
    }
    let title = match limited(p, "title", MAX_TITLE)? {
        t if t.is_empty() => clip.name.chars().take(MAX_TITLE).collect(),
        t => t,
    };
    let raw = cmd::texts(p, "tags");
    let tags = backend::words(raw.iter().map(String::as_str));
    if raw.len() > MAX_TAGS || tags.len() > MAX_TAGS {
        return Err(format!("{MAX_TAGS} tags at most"));
    }
    if let Some(t) = tags.iter().find(|t| t.chars().count() > MAX_TAG) {
        return Err(format!("tag {t:?} is too long ({MAX_TAG} characters at most)"));
    }
    let licence = cmd::text(p, "licence").unwrap_or_default();
    if !LICENCES.contains(&licence) {
        return Err(format!("licence must be one of {}", LICENCES.join(", ")));
    }
    let preview = match cmd::text(p, "preview").filter(|s| !s.trim().is_empty()) {
        None => None,
        Some(path) => Some(read_png(Path::new(path.trim()))?),
    };
    clip.origin.clear();
    let gz = fake::gzip(clipfile::save_clip(&clip).as_bytes())?;
    if gz.len() > MAX_CLIP_GZ {
        return Err("the animation is too big to share (10 MB gzipped)".into());
    }
    let up = Upload {
        title,
        description: limited(p, "description", MAX_DESCRIPTION)?,
        tags,
        licence: licence.into(),
        credit: limited(p, "credit", MAX_CREDIT)?,
        name: limited(p, "name", MAX_NAME)?,
        clip_gz: gz,
        preview_png: preview,
    };
    let clip_id = backend::with(|b| b.share(&up))?;
    Ok(json!({ "clip": clip_id, "status": "checking", "title": up.title, "tags": up.tags }))
}

fn read_png(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("can't read the preview {}: {e}", path.display()))?;
    if bytes.len() > MAX_PREVIEW {
        return Err("the preview is too big (1 MB at most)".into());
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(format!("the preview {} isn't a PNG", path.display()));
    }
    Ok(bytes)
}
