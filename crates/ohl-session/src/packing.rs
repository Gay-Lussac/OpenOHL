// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Long lists of numbers kept in JSON as one string `"zstd+base64:…"`:
//! the bytes (columns of varints, written with [`Writer`]) compressed
//! with zstd and base64-encoded (standard alphabet, padded). Used by the
//! commands changing many LiDAR points ([`crate::points::packed`]) and by
//! saved results holding many points.

use base64::Engine;

/// The prefix of a packed string.
pub const PREFIX: &str = "zstd+base64:";

/// Packs `bytes`.
pub fn pack(bytes: &[u8]) -> Result<String, String> {
    let z = zstd::encode_all(bytes, 3).map_err(|e| e.to_string())?;
    Ok(format!(
        "{PREFIX}{}",
        base64::engine::general_purpose::STANDARD.encode(z)
    ))
}

/// The bytes of a packed string (at most `limit` of them).
pub fn unpack(text: &str, limit: u64) -> Result<Vec<u8>, String> {
    let body = text
        .strip_prefix(PREFIX)
        .ok_or_else(|| format!("not \"{PREFIX}…\""))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    let decoder = zstd::Decoder::new(bytes.as_slice()).map_err(|e| e.to_string())?;
    std::io::Read::read_to_end(&mut std::io::Read::take(decoder, limit), &mut raw)
        .map_err(|e| e.to_string())?;
    Ok(raw)
}

/// Writes numbers as bytes: LEB128 varints, signed ones zigzagged.
#[derive(Debug, Default)]
pub struct Writer(pub Vec<u8>);

impl Writer {
    /// An unsigned number.
    pub fn u(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.0.push((v as u8) | 0x80);
            v >>= 7;
        }
        self.0.push(v as u8);
    }

    /// A signed number.
    pub fn i(&mut self, v: i64) {
        self.u(((v << 1) ^ (v >> 63)) as u64);
    }

    /// A column of signed numbers, each as the difference to the one
    /// before (the first to 0).
    pub fn deltas(&mut self, values: impl IntoIterator<Item = i64>) {
        let mut last = 0i64;
        for v in values {
            self.i(v.wrapping_sub(last));
            last = v;
        }
    }
}

/// Reads what a [`Writer`] wrote.
#[derive(Debug)]
pub struct Reader<'a>(pub &'a [u8]);

impl Reader<'_> {
    /// A byte.
    pub fn byte(&mut self) -> Result<u8, String> {
        let (&b, rest) = self.0.split_first().ok_or("packed data cut short")?;
        self.0 = rest;
        Ok(b)
    }

    /// An unsigned number.
    pub fn u(&mut self) -> Result<u64, String> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.byte()?;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err("packed data: a number too long".into())
    }

    /// A signed number.
    pub fn i(&mut self) -> Result<i64, String> {
        let v = self.u()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }

    /// A column of `n` numbers written by [`Writer::deltas`].
    pub fn deltas(&mut self, n: usize) -> Result<Vec<i64>, String> {
        let mut last = 0i64;
        (0..n)
            .map(|_| {
                last = last.wrapping_add(self.i()?);
                Ok(last)
            })
            .collect()
    }

    /// Whether everything was read.
    pub fn done(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_back() {
        let mut w = Writer::default();
        w.deltas([5, -3, i64::MAX, i64::MIN, 0]);
        w.u(300);
        let text = pack(&w.0).unwrap();
        let raw = unpack(&text, 1 << 20).unwrap();
        let mut r = Reader(&raw);
        assert_eq!(r.deltas(5).unwrap(), [5, -3, i64::MAX, i64::MIN, 0]);
        assert_eq!(r.u().unwrap(), 300);
        assert!(r.done());
        assert!(unpack("zstd+base64:!!", 10).is_err());
        assert!(unpack("plain", 10).is_err());
    }
}
