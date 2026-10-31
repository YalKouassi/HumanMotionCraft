//! The text (ASCII) FBX container, read into the same records as the binary one.
//!
//! Written from the public layout of the format: `; comment` to the end of a line; a record is
//! `Name: value, value, ... { records }`, the values and the block both optional; values are
//! numbers, `"strings"` (`&quot;` stands for a quote) and bare letters, and may run on over lines;
//! `*N { a: v, v, ... }` is an array of N (FBX 7). A name `Class::Name` becomes the binary form
//! `Name\0\x01Class`, so one scene reader serves both. The file is hostile as a binary one is: the
//! same caps, a total of values, and a refusal that names the line.

use super::binary::{
    Document, MAX_ARRAY, MAX_BLOB, MAX_DEPTH, MAX_NODES, MAX_PROPS, MAX_VERSION, MIN_VERSION, Node, Prop,
};

/// Most values (properties and array elements) in one text file: a value in memory is larger than
/// its few bytes of text, so a big file of small numbers would otherwise fill memory.
pub const MAX_VALUES: usize = 64_000_000;

type R<T> = Result<T, String>;

/// Does this look like a text FBX, not a binary one or something else?
pub fn is_text(bytes: &[u8]) -> bool {
    let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let at = b.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(b.len());
    let b = b.get(at..).unwrap_or(&[]);
    b.starts_with(b"; FBX") || b.starts_with(b"FBXHeaderExtension")
}

struct Text<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    nodes: usize,
    values: usize,
}

fn is_delimiter(c: u8) -> bool {
    c.is_ascii_whitespace() || matches!(c, b',' | b'{' | b'}' | b':' | b';' | b'"' | b'*')
}

/// `Class::Name` -> `Name\0\x01Class`, as the binary form holds it. Anything else stays.
fn object_form(s: &str) -> String {
    match s.split_once("::") {
        Some((class, name)) if !class.is_empty() && class.bytes().all(|c| c.is_ascii_alphabetic()) => {
            format!("{name}\u{0}\u{1}{class}")
        }
        _ => s.to_string(),
    }
}

impl Text<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// Past spaces, line ends and comments; whether a line end was crossed.
    fn skip(&mut self) -> bool {
        let mut crossed = false;
        while let Some(c) = self.peek() {
            if c == b'\n' {
                self.line += 1;
                crossed = true;
                self.pos += 1;
            } else if c.is_ascii_whitespace() {
                self.pos += 1;
            } else if c == b';' {
                while self.peek().is_some_and(|c| c != b'\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
        crossed
    }

    fn word(&mut self) -> &str {
        let start = self.pos;
        while self.peek().is_some_and(|c| !is_delimiter(c)) {
            self.pos += 1;
        }
        self.bytes
            .get(start..self.pos)
            .and_then(|b| std::str::from_utf8(b).ok())
            .unwrap_or("")
    }

    /// Is a `:` next on this line?
    fn colon_follows(&self) -> bool {
        let rest = self.bytes.get(self.pos..).unwrap_or(&[]);
        rest.iter()
            .find(|c| !matches!(**c, b' ' | b'\t' | b'\r'))
            .is_some_and(|c| *c == b':')
    }

    fn count_value(&mut self) -> R<()> {
        self.values += 1;
        if self.values > MAX_VALUES {
            return Err(format!(
                "line {}: more than {MAX_VALUES} values in the file",
                self.line
            ));
        }
        Ok(())
    }

    fn string(&mut self) -> R<Prop> {
        let start_line = self.line;
        self.pos += 1;
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c == b'"' {
                break;
            }
            if c == b'\n' {
                self.line += 1;
            }
            self.pos += 1;
        }
        if self.peek().is_none() {
            return Err(format!("truncated: the string on line {start_line} never ends"));
        }
        let raw = self.bytes.get(start..self.pos).unwrap_or(&[]);
        self.pos += 1;
        if raw.len() > MAX_BLOB {
            return Err(format!(
                "line {start_line}: a string of {} bytes is more than {MAX_BLOB}",
                raw.len()
            ));
        }
        let s = String::from_utf8_lossy(raw).replace("&quot;", "\"");
        Ok(Prop::Str(object_form(&s)))
    }

    fn number(&self, token: &str) -> R<Prop> {
        if let Ok(i) = token.parse::<i64>() {
            return Ok(Prop::I64(i));
        }
        match token.parse::<f64>() {
            Ok(v) => Ok(Prop::F64(v)),
            Err(_) => Err(format!("line {}: {token:?} is not a number", self.line)),
        }
    }

    /// `*N { a: v, v, ... }`, at the `*`.
    fn array(&mut self) -> R<Prop> {
        self.pos += 1;
        let line = self.line;
        let count: usize = self
            .word()
            .parse()
            .map_err(|_| format!("line {line}: an array's * has no count"))?;
        if count > MAX_ARRAY {
            return Err(format!(
                "line {line}: an array of {count} values is more than {MAX_ARRAY}"
            ));
        }
        self.skip();
        if self.peek() != Some(b'{') {
            return Err(format!("line {line}: an array's count has no block after it"));
        }
        self.pos += 1;
        self.skip();
        let mut whole: Vec<i64> = Vec::new();
        let mut floats: Option<Vec<f64>> = None;
        let mut seen = 0usize;
        if self.peek() != Some(b'}') {
            if self.word() != "a" || self.peek() != Some(b':') {
                return Err(format!("line {}: an array's block has no a:", self.line));
            }
            self.pos += 1;
            loop {
                self.skip();
                match self.peek() {
                    None => return Err(format!("truncated: the array on line {line} never ends")),
                    Some(b'}') => break,
                    Some(b',') => self.pos += 1,
                    Some(_) => {
                        seen += 1;
                        if seen > count {
                            return Err(format!(
                                "line {line}: an array says {count} values and holds more"
                            ));
                        }
                        self.count_value()?;
                        let at = self.line;
                        let token = self.word().to_string();
                        if token.is_empty() {
                            return Err(format!(
                                "line {at}: an array holds something that is not a number"
                            ));
                        }
                        match self.number(&token)? {
                            Prop::I64(i) => match floats.as_mut() {
                                Some(f) => f.push(i as f64),
                                None => whole.push(i),
                            },
                            Prop::F64(v) => floats
                                .get_or_insert_with(|| whole.iter().map(|i| *i as f64).collect())
                                .push(v),
                            _ => {}
                        }
                    }
                }
            }
        }
        if self.peek() != Some(b'}') {
            return Err(format!("line {line}: an array's block is not closed"));
        }
        self.pos += 1;
        if seen != count {
            return Err(format!(
                "line {line}: an array says {count} values and holds {seen}"
            ));
        }
        Ok(match floats {
            Some(f) => Prop::F64s(f),
            None => Prop::I64s(whole),
        })
    }

    /// One record, at its name.
    fn node(&mut self, depth: usize) -> R<Node> {
        if depth > MAX_DEPTH {
            return Err(format!(
                "line {}: records nest deeper than {MAX_DEPTH}",
                self.line
            ));
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(format!("more than {MAX_NODES} records"));
        }
        let line = self.line;
        let name = self.word().to_string();
        if name.is_empty() || self.peek() != Some(b':') {
            return Err(format!("line {line}: expected a record's name and a colon"));
        }
        self.pos += 1;
        let mut node = Node {
            name,
            props: Vec::new(),
            children: Vec::new(),
        };
        let mut after_comma = false;
        loop {
            let crossed = self.skip();
            let Some(c) = self.peek() else {
                return Ok(node);
            };
            match c {
                b'{' => {
                    self.pos += 1;
                    self.block(&mut node, depth, line)?;
                    return Ok(node);
                }
                b'}' => return Ok(node),
                b',' => {
                    self.pos += 1;
                    after_comma = true;
                    continue;
                }
                _ => {}
            }
            // a value on a new line belongs here only after a comma, or as the record's first
            if crossed && !after_comma && !node.props.is_empty() {
                return Ok(node);
            }
            let value = if c == b'"' {
                self.string()?
            } else if c == b'*' {
                self.array()?
            } else {
                let at = self.pos;
                let token = self.word().to_string();
                if token.is_empty() {
                    return Err(format!("line {}: unexpected {:?}", self.line, c as char));
                }
                if self.colon_follows() {
                    // the next record's name: this one has ended
                    self.pos = at;
                    return Ok(node);
                }
                let first = token.as_bytes().first().copied().unwrap_or(0);
                if first.is_ascii_digit() || matches!(first, b'-' | b'+' | b'.') {
                    self.number(&token)?
                } else {
                    Prop::Str(token)
                }
            };
            self.count_value()?;
            if node.props.len() >= MAX_PROPS {
                return Err(format!(
                    "line {line}: record {:?} holds more than {MAX_PROPS} values",
                    node.name
                ));
            }
            node.props.push(value);
            after_comma = false;
        }
    }

    /// A record's `{ ... }`, past the `{`.
    fn block(&mut self, node: &mut Node, depth: usize, line: usize) -> R<()> {
        loop {
            self.skip();
            match self.peek() {
                None => {
                    return Err(format!(
                        "truncated: the block of {:?} on line {line} never closes",
                        node.name
                    ));
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some(_) => node.children.push(self.node(depth + 1)?),
            }
        }
    }
}

/// The version `; FBX 6.1.0 project file` says.
fn comment_version(bytes: &[u8]) -> Option<u32> {
    let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let line = b
        .split(|c| *c == b'\n')
        .find(|l| l.iter().any(|c| !c.is_ascii_whitespace()))?;
    let line = String::from_utf8_lossy(line);
    let mut parts = line
        .trim()
        .strip_prefix("; FBX ")?
        .split_whitespace()
        .next()?
        .split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    major.checked_mul(1000)?.checked_add(minor.checked_mul(100)?)
}

/// Read a text FBX file's records.
pub fn parse(bytes: &[u8]) -> R<Document> {
    let start = if bytes.starts_with(b"\xEF\xBB\xBF") { 3 } else { 0 };
    let mut t = Text {
        bytes,
        pos: start,
        line: 1,
        nodes: 0,
        values: 0,
    };
    let mut nodes = Vec::new();
    loop {
        t.skip();
        match t.peek() {
            None => break,
            Some(b'}') => return Err(format!("line {}: a }} closes nothing", t.line)),
            Some(_) => nodes.push(t.node(0)?),
        }
    }
    let stated = nodes
        .iter()
        .find(|n| n.name == "FBXHeaderExtension")
        .and_then(|h| h.child("FBXVersion"))
        .and_then(|v| v.i64_at(0))
        .and_then(|v| u32::try_from(v).ok());
    let version = stated
        .or_else(|| comment_version(bytes))
        .ok_or("a text FBX file that does not say its version")?;
    if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
        return Err(format!(
            "FBX version {version} is outside {MIN_VERSION}..={MAX_VERSION}"
        ));
    }
    Ok(Document { version, nodes })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn records_values_arrays_and_comments() {
        let d = parse(
            br#"; FBX 7.4.0 project file
; a comment
FBXHeaderExtension:  {
    FBXVersion: 7400
}
Objects:  {
    Model: 12, "Model::Char::Hips", "LimbNode" {
        Shading: T
        Note: "say &quot;hi&quot;"
    }
    AnimationCurve: 3, "AnimCurve::", "" {
        KeyTime: *3 {
            a: 0,10,
               20
        }
        KeyValueFloat: *2 {
            a: 1,-2.5e-1
        }
        Empty: *0 {
        }
    }
}
"#,
        )
        .unwrap();
        assert_eq!(d.version, 7400);
        let objects = d.nodes.iter().find(|n| n.name == "Objects").unwrap();
        let m = objects.child("Model").unwrap();
        assert_eq!(m.i64_at(0), Some(12));
        // only the class comes off: a namespace stays in the name
        assert_eq!(m.str_at(1), Some("Char::Hips\u{0}\u{1}Model"));
        assert_eq!(m.child("Shading").unwrap().str_at(0), Some("T"));
        assert_eq!(m.child("Note").unwrap().str_at(0), Some("say \"hi\""));
        let c = objects.child("AnimationCurve").unwrap();
        assert_eq!(
            c.child("KeyTime").unwrap().props,
            vec![Prop::I64s(vec![0, 10, 20])]
        );
        assert_eq!(
            c.child("KeyValueFloat").unwrap().props,
            vec![Prop::F64s(vec![1.0, -0.25])]
        );
        assert_eq!(c.child("Empty").unwrap().props, vec![Prop::I64s(vec![])]);
    }

    #[test]
    fn values_run_over_lines_as_fbx_6_writes_them() {
        let d = parse(
            b"; FBX 6.1.0 project file\nA:  {\n  Key: \n    1,2,L,\n    3,4,L\n  Color: 1,1,1\n  Content: , \"ab\"\n    ,\"cd\"\n  Empty: \n  Next: 5\n}\n",
        )
        .unwrap();
        assert_eq!(d.version, 6100);
        let a = &d.nodes[0];
        let key = a.child("Key").unwrap();
        assert_eq!(key.props.len(), 6);
        assert_eq!(key.props[5], Prop::Str("L".into()));
        assert_eq!(a.child("Color").unwrap().props.len(), 3);
        assert_eq!(a.child("Content").unwrap().props.len(), 2);
        assert!(a.child("Empty").unwrap().props.is_empty());
        assert_eq!(a.child("Next").unwrap().i64_at(0), Some(5));
    }

    #[test]
    fn a_file_that_lies_is_refused_with_the_line() {
        let head = "; FBX 7.4.0 project file\n";
        for (body, says) in [
            ("A: *3 {\n a: 1,2\n}\n", "holds 2"),
            ("A: *1 {\n a: 1,2\n}\n", "holds more"),
            ("A: *x {\n}\n", "no count"),
            ("A: *99999999999999999999999999 {\n}\n", "no count"),
            ("A: *100000000 {\n}\n", "more than"),
            ("A: *1 {\n a: 1,", "never ends"),
            ("A: *1 {\n a: nope\n}\n", "not a number"),
            ("A: 1.#INF\n", "not a number"),
            ("A: {\n B: 1\n", "never closes"),
            ("A: \"open\n", "never ends"),
            ("}\n", "closes nothing"),
            ("A B\n", "colon"),
        ] {
            let e = parse(format!("{head}{body}").as_bytes()).unwrap_err();
            assert!(e.contains(says), "{body:?}: {e}");
        }
        // the line is named
        let e = parse(format!("{head}\n\nA: 1.#INF\n").as_bytes()).unwrap_err();
        assert!(e.contains("line 4"), "{e}");
        // deep nesting is refused, not a stack overflow
        let deep = format!("{head}{}", "A: {\n".repeat(MAX_DEPTH + 5));
        assert!(parse(deep.as_bytes()).unwrap_err().contains("nest deeper"));
        // no version, and one out of range
        assert!(
            parse(b"FBXHeaderExtension: {\n}\n")
                .unwrap_err()
                .contains("version")
        );
        let e = parse(b"; FBX 5.0.0 project file\nA: 1\n").unwrap_err();
        assert!(e.contains("outside"), "{e}");
    }

    #[test]
    fn what_counts_as_text() {
        assert!(is_text(b"; FBX 7.4.0 project file\n"));
        assert!(is_text(b"\xEF\xBB\xBF\r\n; FBX 6.1.0 project file\n"));
        assert!(is_text(b"FBXHeaderExtension:  {\n"));
        assert!(!is_text(b"Kaydara FBX Binary  \x00\x1a\x00"));
        assert!(!is_text(b"{\"format\": \"clip\"}"));
        assert!(!is_text(&[0u8; 100]));
        assert!(!is_text(b""));
    }
}
