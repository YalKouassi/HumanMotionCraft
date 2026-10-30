//! The seam between the commands and the server: [`Backend`]. The app and the `ac` binary connect
//! the live server ([`crate::http::Http`]); fixture runs use [`crate::fake::Fake`]; anything else
//! (tests, the extremes check) has none, and every command says the community isn't connected.

use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

pub type R<T> = Result<T, String>;

/// The most search words and tags read.
pub const MAX_WORDS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sort {
    /// Best rated first, then most ratings, then newest.
    Top,
    /// Newest first.
    New,
    /// Most downloaded first, then newest.
    Downloads,
}

/// What `community.browse` asks for. Words and tags are already cleaned ([`words`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    /// Every word must be in the title, the description or a tag.
    pub search: Vec<String>,
    /// Every tag must be one of the clip's.
    pub tags: Vec<String>,
    pub sort: Sort,
    pub page: u32,
    pub limit: u32,
}

/// One published clip, as the server lists it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Listing {
    pub id: String,
    pub title: String,
    pub description: String,
    pub author: String,
    /// Shared by the Official account: the clips that used to come with the app.
    pub official: bool,
    pub tags: Vec<String>,
    pub licence: String,
    pub credit: String,
    pub fps: f64,
    pub frames: i64,
    pub rating: f64,
    pub ratings: i64,
    pub downloads: i64,
    /// When it went live (ISO 8601).
    pub published: String,
    /// Where its preview strip (PNG) is downloaded from (empty: none).
    pub preview: String,
    /// Where the gzipped clip file is downloaded from, and the file's SHA-256.
    pub file: String,
    pub sha256: String,
}

/// A clip to share: its details and the gzipped clip file.
#[derive(Clone, Debug, PartialEq)]
pub struct Upload {
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub licence: String,
    pub credit: String,
    /// The name shown as its author (empty: keep the one the account has).
    pub name: String,
    pub clip_gz: Vec<u8>,
    /// The frames side by side as one PNG strip.
    pub preview_png: Option<Vec<u8>>,
}

/// One of the account's own uploads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Mine {
    pub id: String,
    pub title: String,
    /// checking, published, failed, hidden or removed.
    pub status: String,
    /// Why it failed (empty otherwise).
    pub reason: String,
    pub created: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Account {
    pub signed_in: bool,
    /// Passed the captcha: may share and report.
    pub verified: bool,
    pub name: String,
}

/// The server. Every call may wait on the network; the window makes them off its UI thread.
pub trait Backend: Send {
    fn browse(&mut self, q: &Query) -> R<Vec<Listing>>;
    /// One published clip by id.
    fn listing(&mut self, id: &str) -> R<Listing>;
    /// The gzipped clip file `l.file` points at (the caller checks its hash).
    fn file(&mut self, l: &Listing) -> R<Vec<u8>>;
    /// Counts one install.
    fn counted(&mut self, id: &str) -> R<()>;
    /// Rates a clip 1 to 5 stars; returns its new average and number of ratings.
    fn rate(&mut self, id: &str, stars: u8) -> R<(f64, i64)>;
    fn report(&mut self, id: &str, reason: &str) -> R<()>;
    /// Hands in a captcha token; the account may share and report from then on.
    fn verify(&mut self, token: &str) -> R<()>;
    /// Shares a clip; returns its id (it starts `checking`).
    fn share(&mut self, u: &Upload) -> R<String>;
    fn mine(&mut self) -> R<Vec<Mine>>;
    fn remove(&mut self, id: &str) -> R<()>;
    fn account(&mut self) -> R<Account>;
    /// The captcha page the window opens in the browser.
    fn captcha_page(&mut self) -> R<String>;
}

/// What a refusal from the server says when the account must pass the captcha first. The window
/// sees it in a command's error and opens the captcha page.
pub const CAPTCHA_NEEDED: &str = "captcha needed";

static BACKEND: Mutex<Option<Box<dyn Backend>>> = Mutex::new(None);

fn slot() -> MutexGuard<'static, Option<Box<dyn Backend>>> {
    BACKEND.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Use `b` for every community command from now on.
pub fn use_backend(b: Box<dyn Backend>) {
    *slot() = Some(b);
}

/// Whether a backend is connected.
pub fn connected() -> bool {
    slot().is_some()
}

/// Runs `f` on the backend, or says there is none.
pub fn with<T>(f: impl FnOnce(&mut dyn Backend) -> R<T>) -> R<T> {
    match slot().as_deref_mut() {
        Some(b) => f(b),
        None => Err("the community isn't connected here (the app and `ac` connect to it)".into()),
    }
}

/// Search text or tags as clean words: lower case, letters, digits and `-`, at most
/// [`MAX_WORDS`], no repeats. Anything else separates words, so no text reaches the server's
/// filter syntax.
pub fn words<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in texts {
        for w in t.split(|c: char| !(c.is_alphanumeric() || c == '-')) {
            let w = w.trim_matches('-').to_lowercase();
            if !w.is_empty() && !out.contains(&w) && out.len() < MAX_WORDS {
                out.push(w);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_clean() {
        assert_eq!(
            words(["Wave, HELLO! wave", "a.b(c)", "--x--"]),
            ["wave", "hello", "a", "b", "c", "x"]
        );
        assert_eq!(words(["1 2 3 4 5 6 7 8 9"]).len(), MAX_WORDS);
        assert!(words(["", "  ", "*,()"]).is_empty());
    }
}
