// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Base64 (RFC 4648, standard alphabet, padded): a file carried in a JSON
//! message (phase 12's offline working set).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `bytes` in base64.
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * k)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The bytes of base64 text `text` (refused: not base64).
pub fn decode(text: &str) -> Result<Vec<u8>, String> {
    let value = |c: u8| -> Result<u32, String> {
        Ok(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return Err("not base64".into()),
        }
        .into())
    };
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return Err("not base64 (its length)".into());
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    for chunk in text.chunks(4) {
        let pad = chunk.iter().rev().take_while(|c| **c == b'=').count();
        if pad > 2 {
            return Err("not base64 (its padding)".into());
        }
        let mut n = 0u32;
        for (k, c) in chunk.iter().enumerate() {
            n <<= 6;
            if k < 4 - pad {
                n |= value(*c)?;
            }
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip() {
        for n in 0..70usize {
            let bytes: Vec<u8> = (0..n).map(|k| (k * 37 + 11) as u8).collect();
            let text = super::encode(&bytes);
            assert_eq!(super::decode(&text).unwrap(), bytes);
        }
        assert_eq!(super::encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(super::encode(b"fo"), "Zm8=");
        assert!(super::decode("Zm8").is_err());
        assert!(super::decode("Zm*=").is_err());
    }
}
