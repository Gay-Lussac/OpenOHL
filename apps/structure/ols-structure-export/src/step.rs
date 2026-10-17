// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! STEP physical files (ISO 10303-21, clear text encoding): values,
//! GlobalIds, and the entities of a file being written.

use std::collections::BTreeSet;
use std::fmt::Write as _;

const GUID_CHARS: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// A STEP real (always with a decimal point; -0 written as 0).
pub fn real(x: f64) -> String {
    let x = if x == 0.0 { 0.0 } else { x };
    let s = format!("{x:?}");
    match s.split_once('e') {
        Some((m, e)) => format!("{m}{}E{e}", if m.contains('.') { "" } else { "." }),
        None if s.contains('.') => s,
        None => format!("{s}."),
    }
}

/// A STEP string (quotes doubled, backslashes escaped, other than
/// printable ASCII as `\X2\…\X0\`).
pub fn string(t: &str) -> String {
    let mut out = String::from("'");
    for c in t.chars() {
        match c {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(c),
            c => {
                out.push_str("\\X2\\");
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "{u:04X}");
                }
                out.push_str("\\X0\\");
            }
        }
    }
    out.push('\'');
    out
}

/// An optional STEP string: `$` when empty.
pub fn opt_string(t: &str) -> String {
    if t.is_empty() { "$".into() } else { string(t) }
}

/// An IFC GlobalId (22 characters of the IFC base 64) of a 128-bit number.
pub fn guid(v: u128) -> String {
    let mut s = String::with_capacity(22);
    s.push(GUID_CHARS[((v >> 126) & 3) as usize] as char);
    for k in 1..22 {
        s.push(GUID_CHARS[((v >> (126 - 6 * k)) & 63) as usize] as char);
    }
    s
}

/// A 128-bit hash of a text (two FNV-1a 64 with different bases, each
/// mixed by splitmix64): GlobalIds and SAF ids stay the same from one
/// export to the next.
pub fn hash128(text: &str) -> u128 {
    let fnv = |basis: u64| {
        let h = text.bytes().fold(basis, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
        let mut z = h.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    (u128::from(fnv(0xcbf2_9ce4_8422_2325)) << 64) | u128::from(fnv(0x84222325_cbf29ce4))
}

/// A UUID text (8-4-4-4-12 hex digits) from a hash: SAF's `Id` columns.
pub fn uuid(text: &str) -> String {
    let h = format!("{:032x}", hash128(text));
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// The entities of a file being written, numbered from 1 in order.
pub struct Step {
    /// `TYPE(args)` per entity.
    pub lines: Vec<String>,
    /// Prefix of every GlobalId key (the structure).
    scope: String,
    keys: BTreeSet<String>,
}

impl Step {
    /// An empty file whose GlobalIds derive from `scope` and each key.
    pub fn new(scope: &str) -> Self {
        Self {
            lines: Vec::new(),
            scope: scope.to_owned(),
            keys: BTreeSet::new(),
        }
    }

    /// Adds `TYPE(args)`; returns its reference `#n`.
    pub fn add(&mut self, entity: &str, args: &str) -> String {
        self.lines.push(format!("{entity}({args})"));
        format!("#{}", self.lines.len())
    }

    /// The GlobalId of the object `key` (e.g. `member:m1`), quoted: the same
    /// key in the same structure gives the same id; a key used twice gets
    /// a number.
    pub fn guid(&mut self, key: &str) -> String {
        let mut k = format!("{}/{key}", self.scope);
        let mut n = 2;
        while !self.keys.insert(k.clone()) {
            k = format!("{}/{key}#{n}", self.scope);
            n += 1;
        }
        string(&guid(hash128(&k)))
    }

    /// A 3D point.
    pub fn point(&mut self, p: [f64; 3]) -> String {
        self.add(
            "IFCCARTESIANPOINT",
            &format!("({},{},{})", real(p[0]), real(p[1]), real(p[2])),
        )
    }

    /// A 2D point.
    pub fn point2(&mut self, p: [f64; 2]) -> String {
        self.add(
            "IFCCARTESIANPOINT",
            &format!("({},{})", real(p[0]), real(p[1])),
        )
    }

    /// A direction.
    pub fn direction(&mut self, d: [f64; 3]) -> String {
        self.add(
            "IFCDIRECTION",
            &format!("({},{},{})", real(d[0]), real(d[1]), real(d[2])),
        )
    }

    /// The whole file: header and data section.
    pub fn file(&self, description: &str, name: &str, timestamp: &str, schema: &str) -> String {
        let mut out = format!(
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(({}),'2;1');\n\
             FILE_NAME({},{},(''),(''),{},'OLS CAD','');\n\
             FILE_SCHEMA(('{schema}'));\nENDSEC;\nDATA;\n",
            string(description),
            string(name),
            string(timestamp),
            string(&format!(
                "OLS CAD ols-structure-export {}",
                env!("CARGO_PKG_VERSION")
            )),
        );
        for (k, l) in self.lines.iter().enumerate() {
            let _ = writeln!(out, "#{}={l};", k + 1);
        }
        out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values() {
        assert_eq!(real(1.0), "1.0");
        assert_eq!(real(-0.0), "0.0");
        assert_eq!(real(0.25), "0.25");
        assert_eq!(real(1e-7), "1.E-7");
        assert_eq!(string("it's ä\\"), "'it''s \\X2\\00E4\\X0\\\\\\'");
        assert_eq!(guid(u128::MAX), "3$$$$$$$$$$$$$$$$$$$$$");
        assert_eq!(guid(hash128("a")).len(), 22);
        assert_ne!(hash128("a"), hash128("b"));
        assert_eq!(uuid("x").len(), 36);
        let mut s = Step::new("t");
        let a = s.guid("k");
        let b = s.guid("k");
        assert_ne!(a, b);
        assert_eq!(Step::new("t").guid("k"), a);
    }
}
