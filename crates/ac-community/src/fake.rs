//! The server fixture runs use ([`use_fake`]): fixtures must give the same result on every machine
//! and never reach the network. It holds a few clips whose numbers can be worked out by hand, and
//! keeps the rules the real server keeps (the captcha before sharing and reporting, no rating your
//! own clip, one rating per clip, ratings averaged to two decimals, the same three orders).
//!
//! Each thread has its own state, fresh when the thread starts, so fixtures run side by side
//! don't see each other's ratings or uploads.
//!
//! | id | title | author | tags | licence | fps | frames | rating | ratings | downloads | published |
//! |---|---|---|---|---|---|---|---|---|---|---|
//! | c1 | Happy hop | Ana | jump, happy | CC0-1.0 | 30 | 30 | 4.5 | 2 | 10 | 10-01 |
//! | c2 | Slow walk | Ben | walk | CC-BY-4.0 ("Ben's mocap") | 30 | 90 | 4.5 | 8 | 3 | 10-05 |
//! | c3 | Wave | Cat | gesture, wave | CC0-1.0 | 24 | 48 | 3.0 | 1 | 25 | 10-03 |
//! | c4 | My spin | you | spin | CC0-1.0 | 30 | 60 | 0 | 0 | 1 | 10-04 |
//! | c5 | Broken file | Dee | test | CC0-1.0 | 30 | 30 | 2.0 | 1 | 0 | 10-02 |
//!
//! c5's file doesn't match its hash. The account has also shared "Bad pose" (c6), which failed.
//! The captcha token `pass` passes; any other fails.

use std::cell::RefCell;
use std::io::Write;
use std::sync::Once;

use ac_core::clip::{Clip, Interp, Key};
use ac_core::clipfile;
use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};

use crate::backend::{self, Account, Backend, CAPTCHA_NEEDED, Listing, Mine, Query, R, Sort, Upload};

/// The fake's account id, as the author of its own clips.
const YOU: &str = "You";
/// The date every upload gets (fixtures are deterministic).
const SHARED_ON: &str = "2026-10-07T00:00:00Z";
/// The token the fake's captcha takes.
pub const PASSING_TOKEN: &str = "pass";

struct Entry {
    listing: Listing,
    /// Sum of the other people's stars, and how many.
    stars: (f64, i64),
    yours: Option<u8>,
    status: String,
    reason: String,
    created: String,
}

struct State {
    clips: Vec<Entry>,
    account: Account,
    shared: u32,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(seed());
}

#[allow(clippy::too_many_arguments)]
fn clip(
    id: &str,
    title: &str,
    description: &str,
    author: &str,
    tags: &[&str],
    licence: &str,
    credit: &str,
    (fps, frames): (f64, i64),
    (rating, ratings): (f64, i64),
    downloads: i64,
    day: u32,
) -> Entry {
    let date = format!("2026-10-{day:02}T10:00:00Z");
    Entry {
        listing: Listing {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            author: author.into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            licence: licence.into(),
            credit: credit.into(),
            fps,
            frames,
            rating,
            ratings,
            downloads,
            published: date.clone(),
            preview: format!("fake://previews/{id}.png"),
            file: format!("fake://clips/{id}.clip.json.gz"),
            sha256: String::new(),
        },
        stars: (rating * ratings as f64, ratings),
        yours: None,
        status: "published".into(),
        reason: String::new(),
        created: date,
    }
}

fn seed() -> State {
    let mut failed = clip(
        "c6",
        "Bad pose",
        "",
        YOU,
        &[],
        "CC0-1.0",
        "",
        (30.0, 30),
        (0.0, 0),
        0,
        6,
    );
    failed.status = "failed".into();
    failed.reason = "frame 12: the left knee bends backwards".into();
    failed.listing.published = String::new();
    State {
        clips: vec![
            clip(
                "c1",
                "Happy hop",
                "A small jump for joy",
                "Ana",
                &["jump", "happy"],
                "CC0-1.0",
                "",
                (30.0, 30),
                (4.5, 2),
                10,
                1,
            ),
            clip(
                "c2",
                "Slow walk",
                "Heavy steps",
                "Ben",
                &["walk"],
                "CC-BY-4.0",
                "Ben's mocap",
                (30.0, 90),
                (4.5, 8),
                3,
                5,
            ),
            clip(
                "c3",
                "Wave",
                "A friendly hello",
                "Cat",
                &["gesture", "wave"],
                "CC0-1.0",
                "",
                (24.0, 48),
                (3.0, 1),
                25,
                3,
            ),
            clip(
                "c4",
                "My spin",
                "",
                YOU,
                &["spin"],
                "CC0-1.0",
                "",
                (30.0, 60),
                (0.0, 0),
                1,
                4,
            ),
            clip(
                "c5",
                "Broken file",
                "Its file doesn't match its hash",
                "Dee",
                &["test"],
                "CC0-1.0",
                "",
                (30.0, 30),
                (2.0, 1),
                0,
                2,
            ),
            failed,
        ],
        account: Account::default(),
        shared: 0,
    }
}

/// The fake server, making its clip files for `bones` (the rig's).
pub struct Fake {
    bones: Vec<String>,
}

impl Fake {
    pub fn new(bones: Vec<String>) -> Fake {
        Fake { bones }
    }

    /// The gzipped file of clip `l`: the rig at rest, standing (hips one hip height up), rising a
    /// little half-way so its preview moves.
    fn made(&self, l: &Listing) -> R<Vec<u8>> {
        let mut c = Clip::new(self.bones.clone(), l.fps);
        c.name = l.title.clone();
        let rest = vec![ac_core::quat::IDENTITY; self.bones.len()];
        let last = (l.frames - 1).max(0);
        for (f, up) in [(0, 1.0), (last / 2, 1.15), (last, 1.0)] {
            c.set_key(Key::new(f, rest.clone(), [0.0, 0.0, up], Interp::Linear))?;
        }
        gzip(clipfile::save_clip(&c).as_bytes())
    }
}

pub fn gzip(bytes: &[u8]) -> R<Vec<u8>> {
    let mut z = GzEncoder::new(Vec::new(), Compression::default());
    z.write_all(bytes).map_err(|e| e.to_string())?;
    z.finish().map_err(|e| e.to_string())
}

pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn state<T>(f: impl FnOnce(&mut State) -> R<T>) -> R<T> {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

fn sign_in(s: &mut State) {
    s.account.signed_in = true;
}

fn find<'a>(s: &'a mut State, id: &str) -> R<&'a mut Entry> {
    s.clips
        .iter_mut()
        .find(|c| c.listing.id == id && c.status == "published")
        .ok_or_else(|| format!("no community clip {id:?}"))
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

impl Backend for Fake {
    fn browse(&mut self, q: &Query) -> R<Vec<Listing>> {
        state(|s| {
            let mut found: Vec<Listing> = s
                .clips
                .iter()
                .filter(|c| c.status == "published")
                .map(|c| c.listing.clone())
                .filter(|l| {
                    let text = format!("{} {}", l.title, l.description).to_lowercase();
                    q.search
                        .iter()
                        .all(|w| text.contains(w.as_str()) || l.tags.contains(w))
                        && q.tags.iter().all(|t| l.tags.contains(t))
                })
                .collect();
            let newest = |a: &Listing, b: &Listing| b.published.cmp(&a.published);
            found.sort_by(|a, b| match q.sort {
                Sort::Top => b
                    .rating
                    .total_cmp(&a.rating)
                    .then(b.ratings.cmp(&a.ratings))
                    .then_with(|| newest(a, b)),
                Sort::New => newest(a, b),
                Sort::Downloads => b.downloads.cmp(&a.downloads).then_with(|| newest(a, b)),
            });
            let from = (q.page as usize).saturating_mul(q.limit as usize);
            Ok(found.into_iter().skip(from).take(q.limit as usize).collect())
        })
    }

    fn listing(&mut self, id: &str) -> R<Listing> {
        let mut l = state(|s| Ok(find(s, id)?.listing.clone()))?;
        let gz = self.made(&l)?;
        l.sha256 = if id == "c5" {
            sha256(b"another file")
        } else {
            sha256(&gz)
        };
        Ok(l)
    }

    fn file(&mut self, l: &Listing) -> R<Vec<u8>> {
        self.made(l)
    }

    fn counted(&mut self, id: &str) -> R<()> {
        state(|s| {
            find(s, id)?.listing.downloads += 1;
            Ok(())
        })
    }

    fn rate(&mut self, id: &str, stars: u8) -> R<(f64, i64)> {
        state(|s| {
            sign_in(s);
            let c = find(s, id)?;
            if c.listing.author == YOU {
                return Err("that's your own clip".into());
            }
            c.yours = Some(stars);
            let (sum, n) = c.stars;
            let (sum, n) = (sum + f64::from(stars), n + 1);
            c.listing.rating = round2(sum / n as f64);
            c.listing.ratings = n;
            Ok((c.listing.rating, n))
        })
    }

    fn report(&mut self, id: &str, _: &str) -> R<()> {
        state(|s| {
            sign_in(s);
            if !s.account.verified {
                return Err(CAPTCHA_NEEDED.into());
            }
            let c = find(s, id)?;
            if c.listing.author == YOU {
                return Err("that's your own clip".into());
            }
            Ok(())
        })
    }

    fn verify(&mut self, token: &str) -> R<()> {
        state(|s| {
            sign_in(s);
            if token != PASSING_TOKEN {
                return Err("the captcha didn't pass; try again".into());
            }
            s.account.verified = true;
            Ok(())
        })
    }

    fn share(&mut self, u: &Upload) -> R<String> {
        state(|s| {
            sign_in(s);
            if !s.account.verified {
                return Err(CAPTCHA_NEEDED.into());
            }
            if !u.name.is_empty() {
                s.account.name = u.name.clone();
            }
            s.shared += 1;
            let id = format!("s{}", s.shared);
            let mut c = clip(
                &id,
                &u.title,
                &u.description,
                YOU,
                &[],
                &u.licence,
                &u.credit,
                (30.0, 1),
                (0.0, 0),
                0,
                7,
            );
            c.listing.tags = u.tags.clone();
            c.status = "checking".into();
            c.created = SHARED_ON.into();
            s.clips.push(c);
            Ok(id)
        })
    }

    fn mine(&mut self) -> R<Vec<Mine>> {
        state(|s| {
            sign_in(s);
            let mut mine: Vec<Mine> = s
                .clips
                .iter()
                .filter(|c| c.listing.author == YOU)
                .map(|c| Mine {
                    id: c.listing.id.clone(),
                    title: c.listing.title.clone(),
                    status: c.status.clone(),
                    reason: c.reason.clone(),
                    created: c.created.clone(),
                })
                .collect();
            mine.sort_by(|a, b| b.created.cmp(&a.created));
            Ok(mine)
        })
    }

    fn remove(&mut self, id: &str) -> R<()> {
        state(|s| {
            sign_in(s);
            let c = s
                .clips
                .iter_mut()
                .find(|c| c.listing.id == id && c.listing.author == YOU)
                .ok_or_else(|| format!("no clip {id:?} of yours"))?;
            c.status = "removed".into();
            Ok(())
        })
    }

    fn account(&mut self) -> R<Account> {
        state(|s| Ok(s.account.clone()))
    }

    fn captcha_page(&mut self) -> R<String> {
        Ok("fake://verify.html".into())
    }
}

static ONCE: Once = Once::new();

/// Put the fake in place of the server for the rest of this process (fixture runs only), making
/// its clips for the rig at `rig` (`assets/rig/default_rig.json`).
pub fn use_fake(rig: &std::path::Path) {
    ONCE.call_once(|| {
        let bones = std::fs::read_to_string(rig)
            .ok()
            .and_then(|t| ac_core::Skeleton::from_json(&t).ok())
            .map(|s| s.names)
            .unwrap_or_default();
        backend::use_backend(Box::new(Fake::new(bones)));
    });
}
