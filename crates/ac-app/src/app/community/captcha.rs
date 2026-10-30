//! The captcha, once per account: the app listens on a free port of `127.0.0.1`, opens the
//! captcha page in the browser with that port and a random `state`, and the page sends the browser
//! back to `http://127.0.0.1:<port>/verified?state=…&token=…` once the box is ticked. Only a
//! request with the same `state` counts; the token goes to `community.verify`.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long the app waits for the browser.
const WAIT: Duration = Duration::from_secs(10 * 60);

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Done</title>\
    <body style=\"font:16px system-ui;background:#1e1f22;color:#e6e6e6;display:grid;\
    place-items:center;min-height:90vh\"><p>Done. You can close this tab and go back to the app.</p>";

/// The app waiting for the browser.
pub struct Wait {
    got: mpsc::Receiver<Result<String, String>>,
}

impl Wait {
    /// Opens `page` in the browser and starts listening for its answer.
    pub fn start(page: &str) -> Result<Wait, String> {
        if page.is_empty() {
            return Err("the community server gave no captcha page".into());
        }
        let listener =
            TcpListener::bind("127.0.0.1:0").map_err(|e| format!("can't listen for the browser: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let state = random_state();
        let url = format!("{page}?port={port}&state={state}");
        crate::app::files::open_url(&url)?;
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("community-captcha".into())
            .spawn(move || {
                let _ = tx.send(listen(&listener, &state));
            })
            .map_err(|e| format!("can't start waiting for the browser: {e}"))?;
        Ok(Wait { got: rx })
    }

    /// The token, once the browser came back (none: still waiting).
    pub fn poll(&self) -> Option<Result<String, String>> {
        match self.got.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("stopped waiting for the browser".into())),
        }
    }
}

/// 32 hex digits no other page can guess.
fn random_state() -> String {
    (0..2)
        .map(|i| {
            let mut h = RandomState::new().build_hasher();
            h.write_u64(i);
            h.write_u128(Instant::now().elapsed().as_nanos());
            format!("{:016x}", h.finish())
        })
        .collect()
}

fn listen(listener: &TcpListener, state: &str) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let until = Instant::now() + WAIT;
    while Instant::now() < until {
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(token) = answer(stream, state) {
                    return Ok(token);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("listening for the browser: {e}")),
        }
    }
    Err("the check in the browser wasn't finished in 10 minutes; share or report again to retry".into())
}

/// Reads one request; the token when it is the page's answer with our `state`.
fn answer(mut stream: TcpStream, state: &str) -> Option<String> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).ok()?;
    let head = String::from_utf8_lossy(buf.get(..n)?);
    let target = head.lines().next()?.strip_prefix("GET ")?.split(' ').next()?;
    let token = target.strip_prefix("/verified?").and_then(|q| {
        let get = |k: &str| {
            q.split('&')
                .find_map(|kv| kv.strip_prefix(k).and_then(|v| v.strip_prefix('=')))
                .map(decode)
        };
        (get("state")? == state).then(|| get("token")).flatten()
    });
    let (status, body) = match &token {
        Some(t) if !t.is_empty() => ("200 OK", DONE_PAGE),
        _ => ("400 Bad Request", "not the answer this app is waiting for"),
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    token.filter(|t| !t.is_empty())
}

/// Undoes the page's URL encoding (`%XX`, and `+` for a space).
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        match c {
            b'%' => {
                let hex = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                    None => out.push(c),
                }
            }
            b'+' => out.push(b' '),
            _ => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn only_our_state_counts() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let send = |path: &str| {
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            s
        };
        let _wrong = send("/verified?state=other&token=abc");
        let (stream, _) = l.accept().unwrap();
        assert_eq!(answer(stream, "ours"), None);
        let _right = send("/verified?state=ours&token=a%2Eb-c_d");
        let (stream, _) = l.accept().unwrap();
        assert_eq!(answer(stream, "ours").as_deref(), Some("a.b-c_d"));
    }

    #[test]
    fn states_differ() {
        let (a, b) = (random_state(), random_state());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}
