//! Fuzzy matching for the command palette: a query matches when each of its words' letters
//! appear in order in one of the entry's texts; matches at word starts and runs of letters score
//! higher, and the label counts more than the id, the id more than the docs.

/// How well `query` (one word, any case) matches `text`; `None` when its letters aren't all
/// there in order.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.chars().flat_map(char::to_lowercase).collect();
    let mut qi = 0;
    let mut total = 0;
    let mut run = 0;
    let mut first: Option<usize> = None;
    let mut prev: Option<char> = None;
    for (ti, c) in t.iter().enumerate() {
        let Some(want) = q.get(qi) else { break };
        if c == want {
            let start = prev.is_none_or(|p| !p.is_alphanumeric());
            run += 1;
            total += 10 + if start { 15 } else { 0 } + if run > 1 { 5 * run.min(6) } else { 0 };
            first.get_or_insert(ti);
            qi += 1;
        } else {
            run = 0;
        }
        prev = Some(*c);
    }
    if qi < q.len() {
        return None;
    }
    // earlier is better, and a whole-text match is best
    total -= first.unwrap_or(0).min(20) as i32;
    if q.len() == t.len() {
        total += 50;
    }
    Some(total)
}

/// How well a query of several words matches an entry: every word must match one of `fields`
/// (text, weight in percent); the best field counts for each word.
pub fn score_entry(query: &str, fields: &[(&str, i32)]) -> Option<i32> {
    let mut total = 0;
    for word in query.split_whitespace() {
        let best = fields
            .iter()
            .filter_map(|(text, weight)| score(word, text).map(|s| s * weight / 100))
            .max()?;
        total += best;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_in_order_match() {
        assert!(score("sk", "Set key").is_some());
        assert!(score("ks", "Set key").is_none());
        assert!(score("", "anything").is_some());
        assert!(score("x", "").is_none());
    }

    #[test]
    fn word_starts_and_runs_win() {
        let starts = score("sk", "Set key").unwrap_or(0);
        let inside = score("sk", "Mask").unwrap_or(0);
        assert!(starts > inside, "{starts} {inside}");
        let run = score("undo", "Undo").unwrap_or(0);
        let spread = score("undo", "Unlock door").unwrap_or(0);
        assert!(run > spread);
    }

    #[test]
    fn every_word_must_match_a_field() {
        let fields = [("Move key", 100), ("key.move", 90), ("from: a key's frame", 50)];
        assert!(score_entry("move key", &fields).is_some());
        assert!(score_entry("key.mo", &fields).is_some());
        assert!(score_entry("move zebra", &fields).is_none());
        let label = score_entry("move", &[("Move key", 100)]).unwrap_or(0);
        let doc = score_entry("move", &[("Move key", 50)]).unwrap_or(0);
        assert!(label > doc);
    }
}
