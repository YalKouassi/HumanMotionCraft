//! Reading the training sets' `.npz` files (numpy's zip of `.npy` arrays) without numpy.
//!
//! The small arrays (take names, labels, starts, root offsets) are read whole. The rotations are
//! 1.7 GB for 100STYLE alone, compressed in one stream that can't be read from the middle, so
//! [`unpack`] streams them once into a raw file beside the set (the review cache), which
//! [`crate::store`] then reads a take at a time.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

type R<T> = Result<T, String>;

/// Most a header may be (numpy writes them padded to 64 or 128 bytes).
const MAX_HEADER: usize = 1 << 16;

/// An array's type and shape, from its `.npy` header.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// numpy's type string: `<f2`, `<f4`, `<i8`, `<U22`, `|b1`...
    pub descr: String,
    pub shape: Vec<usize>,
}

impl Header {
    pub fn count(&self) -> usize {
        self.shape.iter().product()
    }
}

fn archive(path: &Path) -> R<zip::ZipArchive<BufReader<File>>> {
    let f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    zip::ZipArchive::new(BufReader::new(f)).map_err(|e| format!("{}: not an npz ({e})", path.display()))
}

/// Read a `.npy` header from the start of a stream; the stream is left at the data.
pub fn header(r: &mut impl Read) -> R<Header> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic).map_err(|e| format!("npy header: {e}"))?;
    if &magic[..6] != b"\x93NUMPY" {
        return Err("not an npy array".into());
    }
    let len = if magic[6] == 1 {
        let mut b = [0u8; 2];
        r.read_exact(&mut b).map_err(|e| format!("npy header: {e}"))?;
        u16::from_le_bytes(b) as usize
    } else {
        let mut b = [0u8; 4];
        r.read_exact(&mut b).map_err(|e| format!("npy header: {e}"))?;
        u32::from_le_bytes(b) as usize
    };
    if len > MAX_HEADER {
        return Err(format!("npy header of {len} bytes"));
    }
    let mut text = vec![0u8; len];
    r.read_exact(&mut text).map_err(|e| format!("npy header: {e}"))?;
    parse_header(&String::from_utf8_lossy(&text))
}

/// `{'descr': '<f2', 'fortran_order': False, 'shape': (3, 104, 4), }`
pub fn parse_header(text: &str) -> R<Header> {
    let field = |name: &str| -> R<&str> {
        let at = text
            .find(&format!("'{name}'"))
            .ok_or_else(|| format!("npy header has no {name}: {text}"))?;
        let rest = text.get(at + name.len() + 2..).unwrap_or("");
        Ok(rest.trim_start().trim_start_matches(':').trim_start())
    };
    let d = field("descr")?;
    let descr = d
        .strip_prefix('\'')
        .and_then(|s| s.split('\'').next())
        .ok_or_else(|| format!("npy descr: {text}"))?
        .to_string();
    if field("fortran_order")?.starts_with("True") {
        return Err("npy arrays in Fortran order are not read".into());
    }
    let s = field("shape")?;
    let inner = s
        .strip_prefix('(')
        .and_then(|s| s.split(')').next())
        .ok_or_else(|| format!("npy shape: {text}"))?;
    let shape = inner
        .split(',')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(|x| x.parse::<usize>().map_err(|e| format!("npy shape {x:?}: {e}")))
        .collect::<R<Vec<_>>>()?;
    Ok(Header { descr, shape })
}

/// One whole array: its header and bytes (for the small ones; capped at `most` bytes).
pub fn read(path: &Path, name: &str, most: usize) -> R<(Header, Vec<u8>)> {
    let mut z = archive(path)?;
    let mut e = z
        .by_name(&format!("{name}.npy"))
        .map_err(|e| format!("{}: no {name} ({e})", path.display()))?;
    let h = header(&mut e)?;
    let mut out = Vec::new();
    e.by_ref()
        .take(most as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("{}: {name}: {e}", path.display()))?;
    if out.len() > most {
        return Err(format!("{}: {name} is over {most} bytes", path.display()));
    }
    Ok((h, out))
}

/// Just an array's header (the rotations' shape without reading them).
pub fn peek(path: &Path, name: &str) -> R<Header> {
    let mut z = archive(path)?;
    let mut e = z
        .by_name(&format!("{name}.npy"))
        .map_err(|e| format!("{}: no {name} ({e})", path.display()))?;
    header(&mut e)
}

/// Stream an array's data (after its header) into `out`; returns its header.
pub fn unpack(path: &Path, name: &str, out: &Path) -> R<Header> {
    let mut z = archive(path)?;
    let mut e = z
        .by_name(&format!("{name}.npy"))
        .map_err(|e| format!("{}: no {name} ({e})", path.display()))?;
    let h = header(&mut e)?;
    let f = File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut w = BufWriter::new(f);
    std::io::copy(&mut e, &mut w).map_err(|e| format!("unpacking {name} to {}: {e}", out.display()))?;
    w.flush().map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(h)
}

fn need(h: &Header, descr: &str, bytes: &[u8], width: usize) -> R<()> {
    if h.descr != descr {
        return Err(format!("expected {descr}, the array is {}", h.descr));
    }
    if bytes.len() != h.count() * width {
        return Err(format!("{} bytes for {} values", bytes.len(), h.count()));
    }
    Ok(())
}

pub fn f32s(h: &Header, bytes: &[u8]) -> R<Vec<f32>> {
    need(h, "<f4", bytes, 4)?;
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect())
}

pub fn i64s(h: &Header, bytes: &[u8]) -> R<Vec<i64>> {
    need(h, "<i8", bytes, 8)?;
    Ok(bytes
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| i64::from_le_bytes(*c))
        .collect())
}

/// numpy's fixed-width unicode (`<U22`: 22 UTF-32 code points each, zero-padded).
pub fn strings(h: &Header, bytes: &[u8]) -> R<Vec<String>> {
    let width: usize = h
        .descr
        .strip_prefix("<U")
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| format!("expected text, the array is {}", h.descr))?;
    if width == 0 || bytes.len() != h.count() * width * 4 {
        return Err(format!(
            "{} bytes for {} texts of {width}",
            bytes.len(),
            h.count()
        ));
    }
    Ok(bytes
        .chunks_exact(width * 4)
        .map(|s| {
            s.as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .take_while(|&u| u != 0)
                .map(|u| char::from_u32(u).unwrap_or('\u{fffd}'))
                .collect()
        })
        .collect())
}

/// IEEE half to single precision.
pub fn f16(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (bits >> 10) & 0x1f;
    let frac = (bits & 0x3ff) as f32;
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        0x1f if frac == 0.0 => sign * f32::INFINITY,
        0x1f => f32::NAN,
        e => sign * (1.0 + frac / 1024.0) * 2f32.powi(e as i32 - 15),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn headers_parse() {
        let h = parse_header("{'descr': '<f2', 'fortran_order': False, 'shape': (3, 104, 4), }").unwrap();
        assert_eq!(
            h,
            Header {
                descr: "<f2".into(),
                shape: vec![3, 104, 4]
            }
        );
        let h = parse_header("{'descr': '<f8', 'fortran_order': False, 'shape': (), }").unwrap();
        assert_eq!((h.shape.len(), h.count()), (0, 1));
        let h = parse_header("{'descr': '<i8', 'fortran_order': False, 'shape': (7,), }").unwrap();
        assert_eq!(h.shape, vec![7]);
        assert!(parse_header("{'descr': '<f4', 'fortran_order': True, 'shape': (2,), }").is_err());
    }

    #[test]
    fn halves_convert() {
        assert_eq!(f16(0x3c00), 1.0);
        assert_eq!(f16(0xc000), -2.0);
        assert_eq!(f16(0x3800), 0.5);
        assert_eq!(f16(0), 0.0);
        assert!((f16(0x3555) - 0.333_25).abs() < 1e-4);
    }

    #[test]
    fn text_arrays_read() {
        let h = Header {
            descr: "<U3".into(),
            shape: vec![2],
        };
        let mut b = Vec::new();
        for s in ["ab", "xyz"] {
            let mut cs: Vec<u32> = s.chars().map(|c| c as u32).collect();
            cs.resize(3, 0);
            for c in cs {
                b.extend(c.to_le_bytes());
            }
        }
        assert_eq!(strings(&h, &b).unwrap(), vec!["ab", "xyz"]);
    }
}
