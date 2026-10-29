//! `cargo xtask names`: other products' names don't appear in the repo (AGENTS.md,
//! "Non-negotiables").
//!
//! The deny-list is kept as hashes, so this file (and the repo) never spells the names it guards
//! against. Text is cut into words (runs of letters, split at lower-to-upper case changes, so
//! `fooBar`, `foo_bar` and `FOO` all give `foo`), each word is lower-cased and hashed with 64-bit
//! FNV-1a, and a hash on the list is a finding. Whole words, not substrings: a short name inside a
//! longer ordinary word is not a hit. File paths are checked the same way as file contents.
//!
//! Two kinds of name:
//! - **products** may appear nowhere;
//! - **credited sources** (a base model whose licence asks for attribution) may appear only where
//!   credits live: the licence notices and the records of where assets and models come from.
//!
//! To add a name: `cargo xtask names --hash <word>` prints its hash; add that number (never the
//! word) to the right list below, with the reason in the commit message.

use std::path::Path;

/// Other products: the hashes of their lower-case names.
const PRODUCTS: &[u64] = &[
    0xa8d1_a70a_4d9a_55d6,
    0x3f29_311e_7f18_7ce9,
    0x8a94_7122_65d9_4534,
];

/// Credited sources (allowed where the licence requires attribution).
const CREDITED: &[u64] = &[0x96bc_b283_b55a_7bf3];

/// Where credited names may appear: the credits and the records of where assets come from.
const CREDIT_FILES: &[&str] = &["ATTRIBUTION.md", "NOTICE", "README.md", "assets/SOURCES.json"];
/// Model cards record the datasets a model was trained on (`weights.json`'s `card.sets`).
const CREDIT_PREFIXES: &[&str] = &["assets/models/"];

/// 64-bit FNV-1a of `word`'s bytes.
pub const fn fnv1a(word: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < word.len() {
        h ^= word[i] as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
        i += 1;
    }
    h
}

/// The lower-cased words of `text`: runs of ASCII letters, split where a lower-case letter is
/// followed by an upper-case one. Yields (byte offset, word).
pub fn words(text: &str) -> impl Iterator<Item = (usize, String)> + '_ {
    let bytes = text.as_bytes();
    let mut i = 0;
    std::iter::from_fn(move || {
        while i < bytes.len() && !bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
        }
        if i >= bytes.len() {
            return None;
        }
        let start = i;
        i += 1;
        while let Some(b) = bytes.get(i) {
            let prev = bytes.get(i - 1).copied().unwrap_or(b' ');
            if !b.is_ascii_alphabetic() || (prev.is_ascii_lowercase() && b.is_ascii_uppercase()) {
                break;
            }
            i += 1;
        }
        let w = bytes
            .get(start..i)
            .map(|w| w.to_ascii_lowercase())
            .unwrap_or_default();
        Some((start, String::from_utf8_lossy(&w).into_owned()))
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Found {
    Product,
    Credited,
}

fn kind(word: &str) -> Option<Found> {
    let h = fnv1a(word.as_bytes());
    if PRODUCTS.contains(&h) {
        Some(Found::Product)
    } else if CREDITED.contains(&h) {
        Some(Found::Credited)
    } else {
        None
    }
}

/// Whether a name of this kind may appear in the file `rel`.
fn allowed(found: Found, rel: &str) -> bool {
    match found {
        Found::Product => false,
        Found::Credited => CREDIT_FILES.contains(&rel) || CREDIT_PREFIXES.iter().any(|p| rel.starts_with(p)),
    }
}

/// The findings in one file's path and text: `path:line: what`.
pub fn scan(rel: &str, text: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(k) = words(rel).find_map(|(_, w)| kind(&w)) {
        // a path is never a credit
        out.push(format!("{rel}: the file name holds a {} name", what(k)));
    }
    let Some(text) = text else { return out };
    for (at, w) in words(text) {
        if let Some(k) = kind(&w).filter(|k| !allowed(*k, rel)) {
            let line = text.get(..at).map_or(0, |t| t.matches('\n').count()) + 1;
            out.push(format!("{rel}:{line}: a {} name ({} letters)", what(k), w.len()));
        }
    }
    out
}

fn what(k: Found) -> &'static str {
    match k {
        Found::Product => "other product's",
        Found::Credited => "credited source's (allowed only in the credits)",
    }
}

/// Check every file of the repo. Returns the findings (empty: clean).
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let mut found = Vec::new();
    for rel in crate::repo::files(root)? {
        let text = crate::repo::text(&root.join(&rel))?;
        found.extend(scan(&rel, text.as_deref()));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn words_split_at_case_and_punctuation() {
        let w: Vec<String> = words("fooBar baz_QUX x2y CamelCase").map(|(_, w)| w).collect();
        assert_eq!(w, ["foo", "bar", "baz", "qux", "x", "y", "camel", "case"]);
    }

    #[test]
    fn hashes_find_whole_words_only() {
        // a word on no list is never a finding, wherever it is
        assert!(scan("src/a.rs", Some("let canny = nanny + uncanny;")).is_empty());
        let fake = fnv1a(b"zzqq");
        assert!(!PRODUCTS.contains(&fake) && !CREDITED.contains(&fake));
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn the_lists_are_disjoint_and_credits_need_a_credit_file() {
        assert!(PRODUCTS.iter().all(|h| !CREDITED.contains(h)));
        assert!(allowed(Found::Credited, "NOTICE"));
        assert!(allowed(Found::Credited, "assets/models/parts/weights.json"));
        assert!(!allowed(Found::Credited, "training/ac_train/quat.py"));
        assert!(!allowed(Found::Product, "README.md"));
    }
}
