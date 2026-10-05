// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Block encoding: integer columns, delta-encoded where sorted, byte
//! planes shuffled (byte 0 of every value, then byte 1, …: neighbouring
//! values share their high bytes, which compress to almost nothing), then
//! zstd.

/// Compressed blobs of a block: coordinates, intensities, returns.
pub type Blobs = (Vec<u8>, Vec<u8>, Vec<u8>);
/// Decoded coordinate columns (x sorted), y, z.
pub type Xyz = (Vec<u32>, Vec<u32>, Vec<u32>);

/// zstd level: 3 compresses ~3× faster than 9 for a few % more size.
const LEVEL: i32 = 3;

fn shuffle(words: &[u32]) -> Vec<u8> {
    let n = words.len();
    let mut out = vec![0u8; 4 * n];
    for (i, w) in words.iter().enumerate() {
        let b = w.to_le_bytes();
        for (plane, byte) in b.iter().enumerate() {
            out[plane * n + i] = *byte;
        }
    }
    out
}

fn unshuffle(bytes: &[u8], n: usize) -> Result<Vec<u32>, String> {
    if bytes.len() != 4 * n {
        return Err(format!("block holds {} bytes for {n} values", bytes.len()));
    }
    Ok((0..n)
        .map(|i| u32::from_le_bytes([bytes[i], bytes[n + i], bytes[2 * n + i], bytes[3 * n + i]]))
        .collect())
}

/// Integer coordinates of a block's points, relative to the block: x and
/// y offsets in the chunk, z offset from the block's lowest z.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Columns {
    /// x offsets (any order on input; sorted ascending on output).
    pub x: Vec<u32>,
    /// y offsets.
    pub y: Vec<u32>,
    /// z offsets.
    pub z: Vec<u32>,
    /// Intensities.
    pub intensity: Vec<u16>,
    /// Return number (high 4 bits) and number of returns (low 4 bits).
    pub returns: Vec<u8>,
}

impl Columns {
    /// Number of points.
    pub fn len(&self) -> usize {
        self.x.len()
    }

    /// Whether there is no point.
    pub fn is_empty(&self) -> bool {
        self.x.is_empty()
    }

    /// Sorts all columns by (x, y).
    pub fn sort(&mut self) {
        let mut order: Vec<usize> = (0..self.len()).collect();
        order.sort_unstable_by_key(|&i| (self.x[i], self.y[i]));
        let pick32 = |v: &[u32]| order.iter().map(|&i| v[i]).collect::<Vec<_>>();
        self.x = pick32(&self.x);
        self.y = pick32(&self.y);
        self.z = pick32(&self.z);
        self.intensity = order.iter().map(|&i| self.intensity[i]).collect();
        self.returns = order.iter().map(|&i| self.returns[i]).collect();
    }

    /// Encodes the coordinates (sorted first) and the attributes: three
    /// compressed blobs (xyz, intensity, returns).
    pub fn encode(&mut self) -> Result<Blobs, String> {
        self.sort();
        let n = self.len();
        let z = |bytes: &[u8]| zstd::encode_all(bytes, LEVEL).map_err(|e| e.to_string());
        let intensity: Vec<u8> = {
            let mut planes = vec![0u8; 2 * n];
            for (i, v) in self.intensity.iter().enumerate() {
                let b = v.to_le_bytes();
                planes[i] = b[0];
                planes[n + i] = b[1];
            }
            planes
        };
        Ok((
            encode_xyz(&self.x, &self.y, &self.z)?,
            z(&intensity)?,
            z(&self.returns)?,
        ))
    }

    /// Decodes the coordinates of `n` points (x sorted ascending).
    pub fn decode_xyz(blob: &[u8], n: usize) -> Result<Xyz, String> {
        // At most the size expected: a damaged block cannot blow up.
        let bytes = zstd::bulk::decompress(blob, 12 * n).map_err(|e| e.to_string())?;
        let words = unshuffle(&bytes, 3 * n)?;
        let mut x = Vec::with_capacity(n);
        let mut acc = 0u32;
        for &d in &words[..n] {
            acc = acc
                .checked_add(d)
                .ok_or("point block: coordinates out of range")?;
            x.push(acc);
        }
        Ok((x, words[n..2 * n].to_vec(), words[2 * n..].to_vec()))
    }

    /// Decodes intensities of `n` points.
    pub fn decode_intensity(blob: &[u8], n: usize) -> Result<Vec<u16>, String> {
        let bytes = zstd::bulk::decompress(blob, 2 * n).map_err(|e| e.to_string())?;
        if bytes.len() != 2 * n {
            return Err("intensity block size".into());
        }
        Ok((0..n)
            .map(|i| u16::from_le_bytes([bytes[i], bytes[n + i]]))
            .collect())
    }

    /// Decodes return numbers of `n` points.
    pub fn decode_returns(blob: &[u8], n: usize) -> Result<Vec<u8>, String> {
        let bytes = zstd::decode_all(blob).map_err(|e| e.to_string())?;
        if bytes.len() != n {
            return Err("returns block size".into());
        }
        Ok(bytes)
    }
}

/// Encodes coordinates already sorted by (x, y) — a block's, or a subset
/// of them in their order: what [`Columns::decode_xyz`] reads.
pub fn encode_xyz(x: &[u32], y: &[u32], z: &[u32]) -> Result<Vec<u8>, String> {
    let n = x.len();
    let mut words = Vec::with_capacity(3 * n);
    let mut prev = 0u32;
    for &v in x {
        words.push(v.checked_sub(prev).ok_or("coordinates not sorted")?);
        prev = v;
    }
    words.extend_from_slice(y);
    words.extend_from_slice(z);
    zstd::encode_all(&shuffle(&words)[..], LEVEL).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_sorted() {
        let mut c = Columns {
            x: vec![30, 10, 20, 10],
            y: vec![1, 2, 3, 0],
            z: vec![5, 6, 7, 8],
            intensity: vec![100, 200, 300, 400],
            returns: vec![0x11, 0x12, 0x22, 0x11],
        };
        let (xyz, i, r) = c.encode().unwrap();
        let (x, y, z) = Columns::decode_xyz(&xyz, 4).unwrap();
        assert_eq!(x, vec![10, 10, 20, 30]);
        assert_eq!(y, vec![0, 2, 3, 1]);
        assert_eq!(z, vec![8, 6, 7, 5]);
        assert_eq!(
            Columns::decode_intensity(&i, 4).unwrap(),
            vec![400, 200, 300, 100]
        );
        assert_eq!(
            Columns::decode_returns(&r, 4).unwrap(),
            vec![0x11, 0x12, 0x22, 0x11]
        );
        assert!(Columns::decode_xyz(&xyz, 5).is_err());
    }
}
