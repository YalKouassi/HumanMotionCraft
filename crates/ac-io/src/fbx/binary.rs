//! The binary FBX container: a header, then node records (name, typed properties, nested
//! records) ended by a null record, then a footer this reader ignores.
//!
//! Written from the public layout of the format: a 23-byte header with the version; records with
//! 32-bit end offsets, property counts and lengths (and 64-bit ones from version 7500 on); scalar
//! properties `Y` i16, `C` bool, `I` i32, `F` f32, `D` f64, `L` i64; arrays `f d l i b` with a
//! count, an encoding (0 raw, 1 zlib) and a byte length; `S` strings and `R` raw bytes with a
//! length. Every number read from the file is hostile: counts are capped, offsets checked against
//! the file, inflated sizes bounded, and a file that lies about itself is refused with where.

/// Most node records in one file.
pub const MAX_NODES: usize = 4_000_000;
/// How deep records may nest.
pub const MAX_DEPTH: usize = 64;
/// Most properties on one record.
pub const MAX_PROPS: usize = 1 << 20;
/// Most elements in one array property.
pub const MAX_ARRAY: usize = 64_000_000;
/// Most bytes one string or raw property may hold.
pub const MAX_BLOB: usize = 64 << 20;
/// The lowest version read: older files lay their records out the same way, but were never checked.
pub const MIN_VERSION: u32 = 6000;
pub const MAX_VERSION: u32 = 9999;

const MAGIC: &[u8] = b"Kaydara FBX Binary  \x00\x1a\x00";

/// One property of a node.
#[derive(Debug, Clone, PartialEq)]
pub enum Prop {
    I16(i16),
    Bool(bool),
    I32(i32),
    F32(f32),
    F64(f64),
    I64(i64),
    Str(String),
    Raw(Vec<u8>),
    F32s(Vec<f32>),
    F64s(Vec<f64>),
    I64s(Vec<i64>),
    I32s(Vec<i32>),
    Bools(Vec<bool>),
}

impl Prop {
    /// A scalar number of any width as f64 (not an array, not text).
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Prop::I16(v) => Some(f64::from(*v)),
            Prop::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            Prop::I32(v) => Some(f64::from(*v)),
            Prop::F32(v) => Some(f64::from(*v)),
            Prop::F64(v) => Some(*v),
            Prop::I64(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// A scalar whole number (a float with no fraction counts).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Prop::I16(v) => Some(i64::from(*v)),
            Prop::Bool(v) => Some(i64::from(*v)),
            Prop::I32(v) => Some(i64::from(*v)),
            Prop::I64(v) => Some(*v),
            Prop::F32(v) if v.fract() == 0.0 && v.is_finite() => Some(*v as i64),
            Prop::F64(v) if v.fract() == 0.0 && v.is_finite() => Some(*v as i64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Prop::Str(s) => Some(s),
            _ => None,
        }
    }

    /// An array of any element type as f64s.
    pub fn as_f64s(&self) -> Option<Vec<f64>> {
        match self {
            Prop::F32s(v) => Some(v.iter().map(|&x| f64::from(x)).collect()),
            Prop::F64s(v) => Some(v.clone()),
            Prop::I64s(v) => Some(v.iter().map(|&x| x as f64).collect()),
            Prop::I32s(v) => Some(v.iter().map(|&x| f64::from(x)).collect()),
            Prop::Bools(v) => Some(v.iter().map(|&x| if x { 1.0 } else { 0.0 }).collect()),
            _ => None,
        }
    }

    /// An array of whole numbers as i64s.
    pub fn as_i64s(&self) -> Option<Vec<i64>> {
        match self {
            Prop::I64s(v) => Some(v.clone()),
            Prop::I32s(v) => Some(v.iter().map(|&x| i64::from(x)).collect()),
            _ => None,
        }
    }
}

/// One node record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Node {
    pub name: String,
    pub props: Vec<Prop>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn prop(&self, i: usize) -> Option<&Prop> {
        self.props.get(i)
    }

    pub fn str_at(&self, i: usize) -> Option<&str> {
        self.props.get(i).and_then(Prop::as_str)
    }

    pub fn i64_at(&self, i: usize) -> Option<i64> {
        self.props.get(i).and_then(Prop::as_i64)
    }

    pub fn f64_at(&self, i: usize) -> Option<f64> {
        self.props.get(i).and_then(Prop::as_f64)
    }
}

/// The file: its version and the top-level records.
#[derive(Debug, Clone)]
pub struct Document {
    pub version: u32,
    pub nodes: Vec<Node>,
}

impl Document {
    pub fn top(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    wide: bool,
    nodes: usize,
}

type R<T> = Result<T, String>;

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, what: &str) -> R<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| format!("{what} at byte {}: length overflows", self.pos))?;
        let s = self.bytes.get(self.pos..end).ok_or_else(|| {
            format!(
                "truncated: {what} at byte {} needs {n} bytes, the file has {}",
                self.pos,
                self.bytes.len()
            )
        })?;
        self.pos = end;
        Ok(s)
    }

    fn u8(&mut self, what: &str) -> R<u8> {
        Ok(self.take(1, what)?.first().copied().unwrap_or(0))
    }

    fn u32(&mut self, what: &str) -> R<u32> {
        let b = self.take(4, what)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self, what: &str) -> R<u64> {
        let b = self.take(8, what)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    /// A record length field: 32 or 64 bits by version.
    fn len(&mut self, what: &str) -> R<u64> {
        if self.wide {
            self.u64(what)
        } else {
            self.u32(what).map(u64::from)
        }
    }

    fn prop(&mut self, what: &str) -> R<Prop> {
        let code = self.u8(what)?;
        Ok(match code {
            b'Y' => {
                let b = self.take(2, what)?;
                Prop::I16(i16::from_le_bytes([b[0], b[1]]))
            }
            b'C' => Prop::Bool(self.u8(what)? != 0),
            b'I' => Prop::I32(self.u32(what)? as i32),
            b'F' => Prop::F32(f32::from_bits(self.u32(what)?)),
            b'D' => Prop::F64(f64::from_bits(self.u64(what)?)),
            b'L' => Prop::I64(self.u64(what)? as i64),
            b'S' | b'R' => {
                let n = self.u32(what)? as usize;
                if n > MAX_BLOB {
                    return Err(format!(
                        "{what}: a {} of {n} bytes is more than {MAX_BLOB}",
                        if code == b'S' { "string" } else { "blob" }
                    ));
                }
                let b = self.take(n, what)?;
                if code == b'S' {
                    Prop::Str(String::from_utf8_lossy(b).into_owned())
                } else {
                    Prop::Raw(b.to_vec())
                }
            }
            b'f' | b'd' | b'l' | b'i' | b'b' => self.array(code, what)?,
            other => {
                return Err(format!(
                    "{what}: unknown property type {:?} at byte {}",
                    other as char,
                    self.pos - 1
                ));
            }
        })
    }

    fn array(&mut self, code: u8, what: &str) -> R<Prop> {
        let count = self.u32(what)? as usize;
        let encoding = self.u32(what)?;
        let stored = self.u32(what)? as usize;
        if count > MAX_ARRAY {
            return Err(format!(
                "{what}: an array of {count} values is more than {MAX_ARRAY}"
            ));
        }
        let width = match code {
            b'f' | b'i' => 4,
            b'd' | b'l' => 8,
            _ => 1,
        };
        let raw_len = count
            .checked_mul(width)
            .ok_or_else(|| format!("{what}: array size overflows"))?;
        let data = self.take(stored, what)?;
        let raw: std::borrow::Cow<[u8]> = match encoding {
            0 => {
                if stored != raw_len {
                    return Err(format!(
                        "{what}: a raw array of {count} values should take {raw_len} bytes, not {stored}"
                    ));
                }
                std::borrow::Cow::Borrowed(data)
            }
            1 => {
                let out = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, raw_len)
                    .map_err(|e| format!("{what}: a compressed array does not inflate ({e})"))?;
                if out.len() != raw_len {
                    return Err(format!(
                        "{what}: a compressed array inflates to {} bytes, not the {raw_len} its count says",
                        out.len()
                    ));
                }
                std::borrow::Cow::Owned(out)
            }
            other => return Err(format!("{what}: unknown array encoding {other}")),
        };
        Ok(match code {
            b'f' => Prop::F32s(
                raw.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| f32::from_le_bytes(*c))
                    .collect(),
            ),
            b'd' => Prop::F64s(
                raw.as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| f64::from_le_bytes(*c))
                    .collect(),
            ),
            b'l' => Prop::I64s(
                raw.as_chunks::<8>()
                    .0
                    .iter()
                    .map(|c| i64::from_le_bytes(*c))
                    .collect(),
            ),
            b'i' => Prop::I32s(
                raw.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| i32::from_le_bytes(*c))
                    .collect(),
            ),
            _ => Prop::Bools(raw.iter().map(|&b| b != 0).collect()),
        })
    }

    /// One record at the current position; `None` for a null record.
    fn node(&mut self, depth: usize) -> R<Option<Node>> {
        let start = self.pos;
        let end = self.len("record end offset")?;
        let nprops = self.len("record property count")?;
        let plen = self.len("record property length")?;
        let name_len = self.u8("record name length")? as usize;
        if end == 0 && nprops == 0 && plen == 0 && name_len == 0 {
            return Ok(None);
        }
        if depth > MAX_DEPTH {
            return Err(format!("records nest deeper than {MAX_DEPTH} at byte {start}"));
        }
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return Err(format!("more than {MAX_NODES} records"));
        }
        let end = usize::try_from(end)
            .map_err(|_| format!("record at byte {start}: end offset {end} overflows"))?;
        if end > self.bytes.len() {
            return Err(format!(
                "truncated or corrupt: record at byte {start} says it ends at byte {end}, past the file's {} bytes",
                self.bytes.len()
            ));
        }
        if end < self.pos {
            return Err(format!(
                "record at byte {start} says it ends at byte {end}, before its own header"
            ));
        }
        let nprops = usize::try_from(nprops).unwrap_or(usize::MAX);
        if nprops > MAX_PROPS {
            return Err(format!("record at byte {start} claims {nprops} properties"));
        }
        let name = String::from_utf8_lossy(self.take(name_len, "record name")?).into_owned();
        let props_start = self.pos;
        let mut props = Vec::with_capacity(nprops.min(64));
        for i in 0..nprops {
            props.push(self.prop(&format!("{name}: property {i}"))?);
        }
        let plen = usize::try_from(plen).unwrap_or(usize::MAX);
        if self.pos - props_start != plen {
            return Err(format!(
                "record {name:?} at byte {start}: its properties take {} bytes, its header says {plen}",
                self.pos - props_start
            ));
        }
        let mut children = Vec::new();
        while self.pos < end {
            match self.node(depth + 1)? {
                Some(c) => children.push(c),
                None => break,
            }
        }
        if self.pos != end {
            return Err(format!(
                "record {name:?} at byte {start}: its records end at byte {}, its header says {end}",
                self.pos
            ));
        }
        Ok(Some(Node {
            name,
            props,
            children,
        }))
    }
}

/// Read a binary FBX file's records.
pub fn parse(bytes: &[u8]) -> R<Document> {
    if bytes.starts_with(b"; FBX") || bytes.starts_with(b"FBXHeaderExtension") {
        return Err("a text (ASCII) FBX file: only binary FBX is read; save it as binary".into());
    }
    if bytes.len() < MAGIC.len() + 4 {
        return Err(format!(
            "not an FBX file: {} bytes is shorter than the header",
            bytes.len()
        ));
    }
    if bytes.get(..MAGIC.len()) != Some(MAGIC) {
        return Err("not an FBX file: the header is not the binary FBX one".into());
    }
    let mut r = Reader {
        bytes,
        pos: MAGIC.len(),
        wide: false,
        nodes: 0,
    };
    let version = r.u32("version")?;
    if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
        return Err(format!(
            "FBX version {version} is outside {MIN_VERSION}..={MAX_VERSION}"
        ));
    }
    r.wide = version >= 7500;
    let mut nodes = Vec::new();
    loop {
        if r.pos >= bytes.len() {
            // the file ends without the closing null record: what was read stands, truncated
            return Err(format!(
                "truncated: the file ends at byte {} inside its records",
                bytes.len()
            ));
        }
        match r.node(0)? {
            Some(n) => nodes.push(n),
            None => break,
        }
    }
    Ok(Document { version, nodes })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn header(version: u32) -> Vec<u8> {
        let mut v = MAGIC.to_vec();
        v.extend_from_slice(&version.to_le_bytes());
        v
    }

    #[test]
    fn refuses_what_is_not_fbx() {
        assert!(parse(b"").unwrap_err().contains("shorter"));
        assert!(parse(b"; FBX 7.4.0 project file\n").unwrap_err().contains("text"));
        assert!(
            parse(b"GLB\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0")
                .unwrap_err()
                .contains("not an FBX")
        );
        let mut v = header(7400);
        v.extend_from_slice(&[0; 13]);
        assert_eq!(parse(&v).unwrap().nodes.len(), 0);
        let mut v = header(3000);
        v.extend_from_slice(&[0; 13]);
        assert!(parse(&v).unwrap_err().contains("version"));
    }

    #[test]
    fn a_record_that_lies_is_refused_with_where() {
        // one record "A" with one I32 property, then the closing null record
        let mut v = header(7400);
        let body: Vec<u8> = {
            let mut b = Vec::new();
            b.push(b'I');
            b.extend_from_slice(&7i32.to_le_bytes());
            b
        };
        let start = v.len();
        let end = start + 13 + 1 + body.len();
        v.extend_from_slice(&(end as u32).to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&(body.len() as u32).to_le_bytes());
        v.push(1);
        v.push(b'A');
        v.extend_from_slice(&body);
        v.extend_from_slice(&[0; 13]);
        let d = parse(&v).unwrap();
        assert_eq!(d.nodes[0].name, "A");
        assert_eq!(d.nodes[0].props, vec![Prop::I32(7)]);
        // truncated inside the record
        let e = parse(&v[..v.len() - 15]).unwrap_err();
        assert!(e.contains("truncated"), "{e}");
        // an end offset past the file
        let mut bad = v.clone();
        bad[start..start + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let e = parse(&bad).unwrap_err();
        assert!(e.contains("ends at byte"), "{e}");
        // a property length that does not match
        let mut bad = v.clone();
        bad[start + 8..start + 12].copy_from_slice(&2u32.to_le_bytes());
        let e = parse(&bad).unwrap_err();
        assert!(e.contains("properties take"), "{e}");
        // an unknown property type
        let mut bad = v.clone();
        bad[start + 14] = b'Q';
        let e = parse(&bad).unwrap_err();
        assert!(e.contains("unknown property type"), "{e}");
    }
}
