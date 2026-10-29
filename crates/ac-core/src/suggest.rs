//! "Did you mean …?": the known names closest to a mistyped one (command ids, bone names).

/// Most names suggested.
pub const MAX_SUGGESTIONS: usize = 3;
/// Names longer than this are not compared (a pasted paragraph is not a typo).
const MAX_LEN: usize = 128;

/// Edit distance (insert, delete, substitute; a swap of two neighbours counts once), ignoring case.
pub fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().flat_map(char::to_lowercase).collect();
    let b: Vec<char> = b.chars().flat_map(char::to_lowercase).collect();
    let w = b.len() + 1;
    // rows i-2, i-1 and i of the table
    let mut prev2 = vec![0usize; w];
    let mut prev: Vec<usize> = (0..w).collect();
    let mut cur = vec![0usize; w];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..w {
            let same = a.get(i - 1) == b.get(j - 1);
            let mut d = (prev[j] + 1)
                .min(cur[j - 1] + 1)
                .min(prev[j - 1] + usize::from(!same));
            if i > 1 && j > 1 && a.get(i - 1) == b.get(j - 2) && a.get(i - 2) == b.get(j - 1) {
                d = d.min(prev2[j - 2] + 1);
            }
            cur[j] = d;
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    prev.get(b.len()).copied().unwrap_or(usize::MAX)
}

/// The known names closest to `wanted`, best first: within a third of its length (at least 2
/// edits), then the ones holding it (`ai` finds `ai-fill`; shortest first), or, failing both, the
/// ones that share its first dotted part (`key.` for `key.sett`).
pub fn closest<'a>(wanted: &str, known: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    if wanted.is_empty() || wanted.chars().count() > MAX_LEN {
        return Vec::new();
    }
    let limit = (wanted.chars().count() / 3).max(2);
    let lower = wanted.to_lowercase();
    let mut scored: Vec<(usize, &str)> = Vec::new();
    let mut holding: Vec<&str> = Vec::new();
    let mut same_area: Vec<&str> = Vec::new();
    let area = lower.split('.').next().unwrap_or_default();
    for k in known {
        if k.chars().count() > MAX_LEN {
            continue;
        }
        let d = distance(wanted, k);
        if d <= limit || k.to_lowercase() == lower {
            scored.push((d, k));
        } else if lower.chars().count() >= 2 && k.to_lowercase().contains(&lower) {
            holding.push(k);
        } else if !area.is_empty() && lower.contains('.') && k.to_lowercase().split('.').next() == Some(area)
        {
            same_area.push(k);
        }
    }
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(b.1)));
    holding.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
    let mut out: Vec<String> = scored
        .into_iter()
        .map(|(_, k)| k)
        .chain(holding)
        .map(str::to_string)
        .collect();
    if out.is_empty() {
        same_area.sort_unstable();
        out = same_area.into_iter().map(str::to_string).collect();
    }
    out.truncate(MAX_SUGGESTIONS);
    out
}

/// " Did you mean …?" for a message, or nothing.
pub fn did_you_mean(suggestions: &[String]) -> String {
    match suggestions {
        [] => String::new(),
        [one] => format!("did you mean {one:?}?"),
        many => format!("did you mean one of {many:?}?"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        assert_eq!(distance("key.set", "key.set"), 0);
        assert_eq!(distance("key.sett", "key.set"), 1);
        assert_eq!(distance("kye.set", "key.set"), 1, "a swap counts once");
        assert_eq!(distance("WRIST.l", "wrist.L"), 0, "case is ignored");
        assert_eq!(distance("", "abc"), 3);
    }

    #[test]
    fn closest_names() {
        let known = ["key.set", "key.move", "keys.delete", "pose.get", "frame.set"];
        assert_eq!(closest("key.sett", known), vec!["key.set"]);
        assert_eq!(closest("key.frobnicate", known), vec!["key.move", "key.set"]);
        assert!(closest("zzzzzzzz", known).is_empty());
        assert!(closest(&"x".repeat(500), known).is_empty());
        assert_eq!(
            closest("wrist.l", ["wrist.L", "wrist.R"]),
            vec!["wrist.L", "wrist.R"]
        );
        // fixture names: near spellings, then the names holding it
        let names = ["clip-move", "clip-move-unkeyed", "ai-fill", "export-glb"];
        assert_eq!(closest("clip-mvoe", names), vec!["clip-move"]);
        assert_eq!(
            closest("clip-move-", names),
            vec!["clip-move", "clip-move-unkeyed"]
        );
        assert_eq!(closest("ai", names), vec!["ai-fill"]);
        assert!(closest("zzzzzz", names).is_empty());
    }
}
