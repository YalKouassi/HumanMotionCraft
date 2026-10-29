//! What the physics pass had to fix, as runs of frames a person can jump to: feet keyed into the
//! ground (lifted out), planted feet that slid (held in place), and still poses off balance (the
//! hips moved over the feet).

use serde::Serialize;

use crate::physics::Report;

/// A run of frames with one kind of problem (`from..=to`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Problem {
    /// "penetration", "sliding" or "unbalanced".
    pub kind: &'static str,
    pub from: usize,
    pub to: usize,
    /// How it reads in a list: "Foot sliding 20–31".
    pub label: String,
}

/// Consecutive frames grouped into `(from, to)` runs. `frames` must be sorted.
fn runs(frames: &[usize]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &f in frames {
        match out.last_mut() {
            Some((_, to)) if f == *to + 1 => *to = f,
            _ => out.push((f, f)),
        }
    }
    out
}

fn span(a: usize, b: usize) -> String {
    if a == b {
        format!("{a}")
    } else {
        format!("{a}–{b}")
    }
}

/// Every problem the pass fixed, ordered by first frame (then kind).
pub fn problems(r: &Report) -> Vec<Problem> {
    let kinds: [(&'static str, &str, &[usize]); 3] = [
        ("penetration", "Feet in the ground", &r.lifted_frames),
        ("sliding", "Foot sliding", &r.slid_frames),
        ("unbalanced", "Off balance", &r.balanced),
    ];
    let mut out: Vec<Problem> = kinds
        .iter()
        .flat_map(|(kind, words, frames)| {
            runs(frames).into_iter().map(move |(a, b)| Problem {
                kind,
                from: a,
                to: b,
                label: format!("{words} {}", span(a, b)),
            })
        })
        .collect();
    out.sort_by(|a, b| (a.from, a.kind).cmp(&(b.from, b.kind)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_group_consecutive_frames() {
        assert_eq!(runs(&[1, 2, 3, 7, 9, 10]), vec![(1, 3), (7, 7), (9, 10)]);
        assert!(runs(&[]).is_empty());
    }

    #[test]
    fn problems_are_labelled_and_ordered() {
        let r = Report {
            rotations: Vec::new(),
            roots: Vec::new(),
            flights: Vec::new(),
            lifted: 2,
            slid: 3,
            balanced: vec![0],
            lifted_frames: vec![4, 5],
            slid_frames: vec![1, 2, 3],
        };
        let p = problems(&r);
        let labels: Vec<&str> = p.iter().map(|x| x.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Off balance 0", "Foot sliding 1–3", "Feet in the ground 4–5"]
        );
    }
}
