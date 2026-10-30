//! The live server: Supabase's REST and functions at [`SERVER`], files from the address its
//! `settings` row gives. The account is anonymous and kept in `<dir>/community.json` (the folder
//! the library sits in), so it belongs to this computer: no email, no password.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ureq::Agent;

use crate::backend::{Account, Backend, CAPTCHA_NEEDED, Listing, Mine, Query, R, Sort, Upload};

/// The project's address and its publishable key: both public, the key made to ship in apps
/// (what it may do is set by the server's row-level security).
pub const SERVER: &str = "https://ksnxwkxaunxutkmdhjzm.supabase.co";
pub const PUBLISHABLE_KEY: &str = include_str!("publishable_key.txt");
/// The account file, in the folder the library sits in.
pub const ACCOUNT_FILE: &str = "community.json";
/// The longest a clip file download may be.
const MAX_FILE: u64 = 12 * 1024 * 1024;

const COLUMNS: &str = "id,title,description,tags,licence,credit,fps,frames,rating_avg,rating_count,\
                       download_count,published_at,preview_key,file_key,sha256,\
                       author:profiles!clips_author_id_fkey(display_name)";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Session {
    user_id: String,
    access_token: String,
    refresh_token: String,
    verified: bool,
    name: String,
}

#[derive(Clone, Debug, Default)]
struct Settings {
    files_url: String,
    captcha_url: String,
}

pub struct Http {
    agent: Agent,
    file: PathBuf,
    session: Option<Session>,
    settings: Option<Settings>,
}

/// A reply: its status and body.
struct Reply {
    status: u16,
    body: Vec<u8>,
}

impl Reply {
    fn json(&self) -> R<Value> {
        serde_json::from_slice(&self.body).map_err(|e| format!("the server's reply isn't JSON: {e}"))
    }

    /// The server's own words for a refusal.
    fn error(&self) -> String {
        let said = self.json().ok().and_then(|v| {
            ["error", "message", "msg"]
                .iter()
                .find_map(|k| v.get(*k).and_then(Value::as_str).map(str::to_string))
        });
        said.unwrap_or_else(|| format!("the server refused ({})", self.status))
    }

    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// Percent-encodes a query value (everything but unreserved characters).
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The REST query `community.browse` sends: published clips, every word in the title,
/// description or tags, every tag, in order, one page. Words are clean ([`crate::backend::words`]).
pub fn browse_query(q: &Query) -> String {
    let mut pairs = vec![
        ("select".to_string(), COLUMNS.to_string()),
        ("status".into(), "eq.published".into()),
    ];
    if !q.search.is_empty() {
        let each: Vec<String> = q
            .search
            .iter()
            .map(|w| format!("or(title.ilike.*{w}*,description.ilike.*{w}*,tags.cs.{{{w}}})"))
            .collect();
        pairs.push(("and".into(), format!("({})", each.join(","))));
    }
    if !q.tags.is_empty() {
        pairs.push(("tags".into(), format!("cs.{{{}}}", q.tags.join(","))));
    }
    let order = match q.sort {
        Sort::Top => "rating_avg.desc,rating_count.desc,published_at.desc",
        Sort::New => "published_at.desc",
        Sort::Downloads => "download_count.desc,published_at.desc",
    };
    pairs.push(("order".into(), order.into()));
    pairs.push(("limit".into(), q.limit.to_string()));
    pairs.push((
        "offset".into(),
        (u64::from(q.page) * u64::from(q.limit)).to_string(),
    ));
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

impl Http {
    /// The server, with the account kept in `dir`.
    pub fn new(dir: PathBuf) -> Http {
        let agent: Agent = Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        let file = dir.join(ACCOUNT_FILE);
        let session = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        Http {
            agent,
            file,
            session,
            settings: None,
        }
    }

    /// Sends a request; `token` signs it as the account (else as the app).
    fn send(
        &self,
        method: &str,
        url: &str,
        token: Option<&str>,
        body: Option<(String, Vec<u8>)>,
    ) -> R<Reply> {
        let bearer = format!("Bearer {}", token.unwrap_or(PUBLISHABLE_KEY.trim()));
        let unreachable = |e: ureq::Error| format!("can't reach the community server: {e}");
        let mut res = match (method, body) {
            ("GET", _) => self
                .agent
                .get(url)
                .header("apikey", PUBLISHABLE_KEY.trim())
                .header("authorization", &bearer)
                .call()
                .map_err(unreachable)?,
            (m, body) => {
                let (kind, bytes) = body.unwrap_or(("application/json".into(), b"{}".to_vec()));
                let req = match m {
                    "PATCH" => self.agent.patch(url),
                    _ => self.agent.post(url),
                };
                req.header("apikey", PUBLISHABLE_KEY.trim())
                    .header("authorization", &bearer)
                    .header("content-type", &kind)
                    .send(&bytes[..])
                    .map_err(unreachable)?
            }
        };
        let status = res.status().as_u16();
        let body = res
            .body_mut()
            .with_config()
            .limit(MAX_FILE)
            .read_to_vec()
            .map_err(|e| format!("the server's reply broke off: {e}"))?;
        Ok(Reply { status, body })
    }

    fn json_body(v: &Value) -> Option<(String, Vec<u8>)> {
        Some(("application/json".into(), v.to_string().into_bytes()))
    }

    fn settings(&mut self) -> R<Settings> {
        if let Some(s) = &self.settings {
            return Ok(s.clone());
        }
        let r = self.send(
            "GET",
            &format!("{SERVER}/rest/v1/settings?select=files_url,captcha_url"),
            None,
            None,
        )?;
        if !r.ok() {
            return Err(r.error());
        }
        let row = r.json()?.get(0).cloned().unwrap_or_default();
        let text = |k: &str| row.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let s = Settings {
            files_url: text("files_url"),
            captcha_url: text("captcha_url"),
        };
        if s.files_url.is_empty() {
            return Err("the community server gave no files address".into());
        }
        self.settings = Some(s.clone());
        Ok(s)
    }

    fn save(&self) -> R<()> {
        let Some(s) = &self.session else { return Ok(()) };
        let text = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
        craft_atomic::atomic_write(&self.file, text.as_bytes())
            .map_err(|e| format!("can't keep the community account in {}: {e}", self.file.display()))
    }

    /// Takes the tokens an auth reply gives.
    fn took(&mut self, r: &Reply) -> R<()> {
        let v = r.json()?;
        let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let user = v
            .get("user")
            .and_then(|u| u.get("id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut s = self.session.clone().unwrap_or_default();
        if !user.is_empty() && user != s.user_id {
            s = Session {
                user_id: user.into(),
                ..Session::default()
            };
        }
        s.access_token = text("access_token");
        s.refresh_token = text("refresh_token");
        if s.access_token.is_empty() || s.user_id.is_empty() {
            return Err("the community server didn't sign in".into());
        }
        self.session = Some(s);
        self.save()
    }

    /// A new anonymous account for this computer.
    fn sign_in(&mut self) -> R<()> {
        let r = self.send(
            "POST",
            &format!("{SERVER}/auth/v1/signup"),
            None,
            Self::json_body(&json!({})),
        )?;
        if !r.ok() {
            return Err(format!("can't make a community account: {}", r.error()));
        }
        self.took(&r)
    }

    /// New tokens for the account; false when its refresh token no longer works.
    fn refresh(&mut self) -> R<bool> {
        let Some(token) = self.session.as_ref().map(|s| s.refresh_token.clone()) else {
            return Ok(false);
        };
        let r = self.send(
            "POST",
            &format!("{SERVER}/auth/v1/token?grant_type=refresh_token"),
            None,
            Self::json_body(&json!({ "refresh_token": token })),
        )?;
        if !r.ok() {
            return Ok(false);
        }
        self.took(&r).map(|_| true)
    }

    /// Sends as the account (making one first if there is none); a stale token is refreshed and
    /// the request sent again once.
    fn authed(&mut self, method: &str, url: &str, body: Option<(String, Vec<u8>)>) -> R<Reply> {
        if self.session.is_none() {
            self.sign_in()?;
        }
        let token = self
            .session
            .as_ref()
            .map(|s| s.access_token.clone())
            .unwrap_or_default();
        let r = self.send(method, url, Some(&token), body.clone())?;
        if r.status != 401 {
            return Ok(r);
        }
        if !self.refresh()? {
            return Err("the community account on this computer no longer signs in".into());
        }
        let token = self
            .session
            .as_ref()
            .map(|s| s.access_token.clone())
            .unwrap_or_default();
        self.send(method, url, Some(&token), body)
    }

    fn user(&self) -> String {
        self.session
            .as_ref()
            .map(|s| s.user_id.clone())
            .unwrap_or_default()
    }

    /// Calls a function; a captcha refusal clears the account's verified mark.
    fn function(&mut self, name: &str, body: Option<(String, Vec<u8>)>) -> R<Value> {
        let r = self.authed("POST", &format!("{SERVER}/functions/v1/{name}"), body)?;
        if r.ok() {
            return r.json();
        }
        let e = r.error();
        if e == CAPTCHA_NEEDED {
            if let Some(s) = &mut self.session {
                s.verified = false;
            }
            self.save()?;
        }
        Err(e)
    }

    fn listing(&mut self, row: &Value) -> R<Listing> {
        let files = self.settings()?.files_url;
        let text = |k: &str| row.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let num = |k: &str| row.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let int = |k: &str| row.get(k).and_then(Value::as_i64).unwrap_or(0);
        let preview = text("preview_key");
        Ok(Listing {
            id: text("id"),
            title: text("title"),
            description: text("description"),
            author: row
                .get("author")
                .and_then(|a| a.get("display_name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            tags: row
                .get("tags")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
            licence: text("licence"),
            credit: text("credit"),
            fps: num("fps"),
            frames: int("frames"),
            rating: num("rating_avg"),
            ratings: int("rating_count"),
            downloads: int("download_count"),
            published: text("published_at"),
            preview: if preview.is_empty() {
                String::new()
            } else {
                format!("{files}/{preview}")
            },
            file: format!("{files}/{}", text("file_key")),
            sha256: text("sha256"),
        })
    }
}

fn multipart(u: &Upload) -> R<(String, Vec<u8>)> {
    let boundary = "----community-upload-7f3a9c2e";
    let meta = json!({
        "title": u.title, "description": u.description, "tags": u.tags,
        "licence": u.licence, "credit": u.credit, "display_name": u.name,
    });
    let mut body = Vec::new();
    let mut part = |name: &str, file: Option<(&str, &str)>, bytes: &[u8]| {
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        match file {
            Some((filename, kind)) => body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\n\
                     Content-Type: {kind}\r\n\r\n"
                )
                .as_bytes(),
            ),
            None => body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
            ),
        }
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    };
    part("meta", None, meta.to_string().as_bytes());
    part("clip", Some(("clip.json.gz", "application/gzip")), &u.clip_gz);
    if let Some(png) = &u.preview_png {
        part("preview", Some(("preview.png", "image/png")), png);
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok((format!("multipart/form-data; boundary={boundary}"), body))
}

impl Backend for Http {
    fn browse(&mut self, q: &Query) -> R<Vec<Listing>> {
        let r = self.send(
            "GET",
            &format!("{SERVER}/rest/v1/clips?{}", browse_query(q)),
            None,
            None,
        )?;
        if !r.ok() {
            return Err(r.error());
        }
        let rows = r.json()?;
        let rows = rows.as_array().cloned().unwrap_or_default();
        rows.iter().map(|row| self.listing(row)).collect()
    }

    fn listing(&mut self, id: &str) -> R<Listing> {
        let url = format!(
            "{SERVER}/rest/v1/clips?select={}&status=eq.published&id=eq.{}",
            enc(COLUMNS),
            enc(id)
        );
        let r = self.send("GET", &url, None, None)?;
        if !r.ok() {
            return Err(r.error());
        }
        let row = r.json()?.get(0).cloned();
        match row {
            Some(row) => Http::listing(self, &row),
            None => Err(format!("no community clip {id:?}")),
        }
    }

    fn file(&mut self, l: &Listing) -> R<Vec<u8>> {
        let mut res = self
            .agent
            .get(&l.file)
            .call()
            .map_err(|e| format!("can't download the clip: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("can't download the clip ({})", res.status().as_u16()));
        }
        res.body_mut()
            .with_config()
            .limit(MAX_FILE)
            .read_to_vec()
            .map_err(|e| format!("the download broke off: {e}"))
    }

    fn counted(&mut self, id: &str) -> R<()> {
        let r = self.send(
            "POST",
            &format!("{SERVER}/rest/v1/rpc/count_download"),
            None,
            Self::json_body(&json!({ "clip": id })),
        )?;
        if r.ok() { Ok(()) } else { Err(r.error()) }
    }

    fn rate(&mut self, id: &str, stars: u8) -> R<(f64, i64)> {
        if self.session.is_none() {
            self.sign_in()?;
        }
        let user = self.user();
        let row = json!({ "clip_id": id, "user_id": user, "stars": stars });
        let r = self.authed(
            "POST",
            &format!("{SERVER}/rest/v1/ratings"),
            Self::json_body(&row),
        )?;
        if r.status == 409 {
            let url = format!(
                "{SERVER}/rest/v1/ratings?clip_id=eq.{}&user_id=eq.{}",
                enc(id),
                enc(&user)
            );
            let again = self.authed("PATCH", &url, Self::json_body(&json!({ "stars": stars })))?;
            if !again.ok() {
                return Err(again.error());
            }
        } else if r.status == 403 {
            return Err("you can't rate that clip (your own, or not published)".into());
        } else if !r.ok() {
            return Err(r.error());
        }
        let l = Backend::listing(self, id)?;
        Ok((l.rating, l.ratings))
    }

    fn report(&mut self, id: &str, reason: &str) -> R<()> {
        self.function("report", Self::json_body(&json!({ "id": id, "reason": reason })))
            .map(|_| ())
    }

    fn verify(&mut self, token: &str) -> R<()> {
        self.function("verify", Self::json_body(&json!({ "token": token })))?;
        if let Some(s) = &mut self.session {
            s.verified = true;
        }
        self.save()
    }

    fn share(&mut self, u: &Upload) -> R<String> {
        let (kind, body) = multipart(u)?;
        let v = self.function("upload", Some((kind, body)))?;
        if !u.name.is_empty()
            && let Some(s) = &mut self.session
        {
            s.name = u.name.clone();
            self.save()?;
        }
        v.get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "the server didn't say the clip's id".into())
    }

    fn mine(&mut self) -> R<Vec<Mine>> {
        if self.session.is_none() {
            return Ok(Vec::new());
        }
        let url = format!(
            "{SERVER}/rest/v1/clips?select=id,title,status,fail_reason,created_at&author_id=eq.{}&order=created_at.desc",
            enc(&self.user())
        );
        let r = self.authed("GET", &url, None)?;
        if !r.ok() {
            return Err(r.error());
        }
        let rows = r.json()?.as_array().cloned().unwrap_or_default();
        Ok(rows
            .iter()
            .map(|row| {
                let text = |k: &str| row.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
                Mine {
                    id: text("id"),
                    title: text("title"),
                    status: text("status"),
                    reason: text("fail_reason"),
                    created: text("created_at"),
                }
            })
            .collect())
    }

    fn remove(&mut self, id: &str) -> R<()> {
        self.function("remove", Self::json_body(&json!({ "id": id })))
            .map(|_| ())
    }

    fn account(&mut self) -> R<Account> {
        Ok(match &self.session {
            None => Account::default(),
            Some(s) => Account {
                signed_in: true,
                verified: s.verified,
                name: s.name.clone(),
            },
        })
    }

    fn captcha_page(&mut self) -> R<String> {
        Ok(self.settings()?.captcha_url)
    }
}

/// A preview strip (or any public file) from `url`, 2 MB at most.
pub fn get_bytes(url: &str) -> R<Vec<u8>> {
    let agent: Agent = Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into();
    let mut res = agent
        .get(url)
        .call()
        .map_err(|e| format!("can't download {url}: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("can't download {url} ({})", res.status().as_u16()));
    }
    res.body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("the download broke off: {e}"))
}

/// Connect the live server for the rest of this process, keeping the account in `dir`.
pub fn connect(dir: PathBuf) {
    crate::backend::use_backend(Box::new(Http::new(dir)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browse_query_reads_as_postgrest_wants() {
        let q = Query {
            search: vec!["wave".into(), "hello".into()],
            tags: vec!["gesture".into()],
            sort: Sort::Downloads,
            page: 2,
            limit: 10,
        };
        let s = browse_query(&q);
        assert!(s.contains("status=eq.published"));
        assert!(s.contains(&format!(
            "and={}",
            enc("(or(title.ilike.*wave*,description.ilike.*wave*,tags.cs.{wave}),or(title.ilike.*hello*,description.ilike.*hello*,tags.cs.{hello}))")
        )));
        assert!(s.contains(&format!("tags={}", enc("cs.{gesture}"))));
        assert!(s.contains(&format!("order={}", enc("download_count.desc,published_at.desc"))));
        assert!(s.ends_with("limit=10&offset=20"));
    }

    #[test]
    fn values_are_encoded() {
        assert_eq!(enc("a b,{c}*"), "a%20b%2C%7Bc%7D%2A");
        assert_eq!(enc("ok-._~9"), "ok-._~9");
    }
}
