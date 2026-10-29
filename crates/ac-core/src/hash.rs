//! Content hashes for `session.info`: which rig, limits and model a session runs with, so two runs
//! can be told apart (or proved alike) without comparing files. FNV-1a, 64 bits: deterministic on
//! every platform, no dependency, and enough to tell versions apart (not a security hash).

use crate::fullbody::Limits;
use crate::rig::Skeleton;

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// A running FNV-1a hash.
#[derive(Debug, Clone, Copy)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Fnv(OFFSET)
    }
}

impl Fnv {
    pub fn bytes(mut self, data: &[u8]) -> Fnv {
        for b in data {
            self.0 ^= u64::from(*b);
            self.0 = self.0.wrapping_mul(PRIME);
        }
        self
    }

    pub fn text(self, s: &str) -> Fnv {
        // the length first, so ["ab", "c"] and ["a", "bc"] differ
        self.bytes(&(s.len() as u64).to_le_bytes()).bytes(s.as_bytes())
    }

    pub fn number(self, x: f64) -> Fnv {
        self.bytes(&x.to_bits().to_le_bytes())
    }

    /// The hash as 16 hex digits.
    pub fn hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

/// The hash of a byte string, as 16 hex digits.
pub fn of_bytes(data: &[u8]) -> String {
    Fnv::default().bytes(data).hex()
}

/// The rig: bone names, parents, rest heads and the hip height.
pub fn skeleton(sk: &Skeleton) -> String {
    let mut h = Fnv::default().number(sk.hip_height);
    for (i, n) in sk.names.iter().enumerate() {
        h = h.text(n);
        let parent = sk.parents.get(i).copied().flatten().map_or(-1.0, |p| p as f64);
        h = h.number(parent);
        for x in sk.heads.get(i).copied().unwrap_or_default() {
            h = h.number(x);
        }
    }
    h.hex()
}

/// The joint limits, bone by bone.
pub fn limits(l: &Limits) -> String {
    let mut h = Fnv::default();
    for v in l.lo.iter().chain(&l.hi) {
        for x in v {
            h = h.number(*x);
        }
    }
    h.hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        // FNV-1a 64 reference values
        assert_eq!(of_bytes(b""), "cbf29ce484222325");
        assert_eq!(of_bytes(b"a"), "af63dc4c8601ec8c");
        assert_ne!(
            Fnv::default().text("ab").text("c").hex(),
            Fnv::default().text("a").text("bc").hex()
        );
    }
}
