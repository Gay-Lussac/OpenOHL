// This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
// License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.

//! Every number of a value finite: a NaN or an infinity written as JSON
//! (the project file) becomes `null` and no longer reads as a number, so a
//! line holding one is refused before it is stored ([`crate::Line::validate`]).
//! The value is walked as it would be written (its `Serialize`), nothing
//! written.

use serde::Serialize;
use serde::ser;

/// Why a value was refused: the first number that is not finite, with
/// where it is (the fields and indices leading to it).
#[derive(Debug)]
pub struct NotFinite(pub String);

impl std::fmt::Display for NotFinite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NotFinite {}

impl ser::Error for NotFinite {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        NotFinite(msg.to_string())
    }
}

/// Checks that every number of `value` is finite; where the first one that
/// is not lies, in words (`sections[3].stringing.horizontal_tension`).
///
/// Cost: one walk through the value without writing anything; a second
/// one, keeping track of where it is, only when a number is not finite.
pub fn check(value: &impl Serialize) -> Result<(), String> {
    let quick = Walk {
        at: String::new(),
        track: false,
    };
    if value.serialize(quick).is_ok() {
        return Ok(());
    }
    value
        .serialize(Walk {
            at: String::new(),
            track: true,
        })
        .map_err(|e| e.0)
}

/// The walker, at a place of the value (kept only when `track`).
struct Walk {
    at: String,
    track: bool,
}

impl Walk {
    fn at_part(&self, part: &str) -> Walk {
        if !self.track {
            return Walk {
                at: String::new(),
                track: false,
            };
        }
        Walk {
            track: true,
            at: if self.at.is_empty() {
                part.to_owned()
            } else if part.starts_with('[') {
                format!("{}{part}", self.at)
            } else {
                format!("{}.{part}", self.at)
            },
        }
    }

    fn number(&self, v: f64) -> Result<(), NotFinite> {
        if v.is_finite() {
            Ok(())
        } else {
            Err(NotFinite(format!(
                "{} is {v}: every number must be finite",
                if self.at.is_empty() {
                    "a number"
                } else {
                    &self.at
                }
            )))
        }
    }
}

/// A sequence, tuple or map being walked: the place, the next index.
struct Many {
    at: Walk,
    k: usize,
}

impl Many {
    fn element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), NotFinite> {
        let w = if self.at.track {
            self.at.at_part(&format!("[{}]", self.k))
        } else {
            self.at.at_part("")
        };
        self.k += 1;
        v.serialize(w)
    }
}

impl ser::Serializer for Walk {
    type Ok = ();
    type Error = NotFinite;
    type SerializeSeq = Many;
    type SerializeTuple = Many;
    type SerializeTupleStruct = Many;
    type SerializeTupleVariant = Many;
    type SerializeMap = Many;
    type SerializeStruct = Many;
    type SerializeStructVariant = Many;

    fn serialize_bool(self, _: bool) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_i8(self, _: i8) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_i16(self, _: i16) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_i32(self, _: i32) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_i64(self, _: i64) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_u8(self, _: u8) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_u16(self, _: u16) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_u32(self, _: u32) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_u64(self, _: u64) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_f32(self, v: f32) -> Result<(), NotFinite> {
        self.number(f64::from(v))
    }
    fn serialize_f64(self, v: f64) -> Result<(), NotFinite> {
        self.number(v)
    }
    fn serialize_char(self, _: char) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_str(self, _: &str) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_none(self) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_some<T: ?Sized + Serialize>(self, v: &T) -> Result<(), NotFinite> {
        v.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_unit_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
    ) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        v: &T,
    ) -> Result<(), NotFinite> {
        v.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<(), NotFinite> {
        v.serialize(self.at_part(variant))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Many, NotFinite> {
        Ok(Many { at: self, k: 0 })
    }
    fn serialize_tuple(self, _: usize) -> Result<Many, NotFinite> {
        Ok(Many { at: self, k: 0 })
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Many, NotFinite> {
        Ok(Many { at: self, k: 0 })
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Many, NotFinite> {
        Ok(Many {
            at: self.at_part(variant),
            k: 0,
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Many, NotFinite> {
        Ok(Many { at: self, k: 0 })
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Many, NotFinite> {
        Ok(Many { at: self, k: 0 })
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Many, NotFinite> {
        Ok(Many {
            at: self.at_part(variant),
            k: 0,
        })
    }
}

impl ser::SerializeSeq for Many {
    type Ok = ();
    type Error = NotFinite;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), NotFinite> {
        self.element(v)
    }
    fn end(self) -> Result<(), NotFinite> {
        Ok(())
    }
}

impl ser::SerializeTuple for Many {
    type Ok = ();
    type Error = NotFinite;
    fn serialize_element<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), NotFinite> {
        self.element(v)
    }
    fn end(self) -> Result<(), NotFinite> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for Many {
    type Ok = ();
    type Error = NotFinite;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), NotFinite> {
        self.element(v)
    }
    fn end(self) -> Result<(), NotFinite> {
        Ok(())
    }
}

impl ser::SerializeTupleVariant for Many {
    type Ok = ();
    type Error = NotFinite;
    fn serialize_field<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), NotFinite> {
        self.element(v)
    }
    fn end(self) -> Result<(), NotFinite> {
        Ok(())
    }
}

impl ser::SerializeMap for Many {
    type Ok = ();
    type Error = NotFinite;
    fn serialize_key<T: ?Sized + Serialize>(&mut self, _: &T) -> Result<(), NotFinite> {
        Ok(())
    }
    fn serialize_value<T: ?Sized + Serialize>(&mut self, v: &T) -> Result<(), NotFinite> {
        self.element(v)
    }
    fn end(self) -> Result<(), NotFinite> {
        Ok(())
    }
}

impl ser::SerializeStruct for Many {
    type Ok = ();
    type Error = NotFinite;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), NotFinite> {
        v.serialize(self.at.at_part(key))
    }
    fn end(self) -> Result<(), NotFinite> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for Many {
    type Ok = ();
    type Error = NotFinite;
    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        v: &T,
    ) -> Result<(), NotFinite> {
        v.serialize(self.at.at_part(key))
    }
    fn end(self) -> Result<(), NotFinite> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct S {
        a: f64,
        b: Vec<[f64; 2]>,
        c: Option<f64>,
    }

    /// Finite numbers pass; a NaN or an infinity is refused, said where.
    #[test]
    fn a_number_not_finite_is_found_where_it_is() {
        let ok = S {
            a: 1.0,
            b: vec![[2.0, 3.0]],
            c: None,
        };
        assert!(check(&ok).is_ok());
        let bad = S {
            a: 1.0,
            b: vec![[2.0, 3.0], [4.0, f64::NAN]],
            c: Some(1.0),
        };
        assert_eq!(
            check(&bad).unwrap_err(),
            "b[1][1] is NaN: every number must be finite"
        );
        let inf = S {
            a: 1.0,
            b: Vec::new(),
            c: Some(f64::INFINITY),
        };
        assert!(check(&inf).unwrap_err().starts_with("c is inf"));
    }
}
