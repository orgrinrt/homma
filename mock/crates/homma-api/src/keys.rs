//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital

//! Every key a deserialisable type reads, found with no manifest.
//!
//! [`keys_of`] deserialises the type from a probe. The derive hands the probe
//! the names of a struct's fields; the probe records each and descends into its
//! value, writing a map's keys `*` and an array's elements under the array's own
//! name. A value read as any value (`deserialize_any`: a raw `toml::Value`, an
//! untagged enum) shows no shape, so its key is listed and held apart as opaque.

use std::cell::RefCell;
use std::collections::BTreeSet;

use serde::de::value::{Error, StrDeserializer};
use serde::de::{
    self,
    DeserializeOwned,
    DeserializeSeed,
    EnumAccess,
    IntoDeserializer,
    MapAccess,
    SeqAccess,
    VariantAccess,
    Visitor,
};

/// The keys a type reads, as dotted paths, and which of them are opaque.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keys {
    read:   BTreeSet<String>,
    opaque: BTreeSet<String>,
}

impl Keys {
    /// Every key read, the opaque ones included.
    pub fn read(&self) -> &BTreeSet<String> {
        &self.read
    }

    /// The keys read as any value, whose keys under them, if any, are not seen.
    pub fn opaque(&self) -> &BTreeSet<String> {
        &self.opaque
    }

    /// Puts the keys of the type that reads `at` in place of it: `other`'s
    /// keys go in under `at`, or at the root where `at` is empty.
    ///
    /// An opaque key is then covered, and leaves both sets, where a key now
    /// lies under it, or where `other` reads it as a plain key. A key `other`
    /// holds opaque stays opaque.
    pub fn graft(&mut self, at: &str, other: Keys) {
        let under = |k: String| if at.is_empty() { k } else { format!("{at}.{k}") };
        let plain: BTreeSet<String> = other
            .read
            .difference(&other.opaque)
            .cloned()
            .map(under)
            .collect();
        let opaque: BTreeSet<String> = other.opaque.into_iter().map(under).collect();
        self.read.extend(plain.iter().cloned());
        self.read.extend(opaque.iter().cloned());
        let covered: Vec<String> = self
            .opaque
            .iter()
            .filter(|k| {
                !opaque.contains(*k)
                    && (plain.contains(*k)
                        || self.read.iter().any(|r| {
                            r.strip_prefix(k.as_str())
                                .is_some_and(|rest| rest.starts_with('.'))
                        }))
            })
            .cloned()
            .collect();
        for k in covered {
            self.opaque.remove(&k);
            if !plain.contains(&k) {
                self.read.remove(&k);
            }
        }
        self.opaque.extend(opaque);
    }
}

/// The keys `T` reads, or why the probe could not read it.
///
/// A leaf is offered the plainest value of its kind: `false`, `1`, `"x"`, and a
/// map the one key `*`. A type whose own check refuses what the probe offered
/// at a key is probed again with that key empty, a map or an array with no
/// entry and an option with none, keeping the keys the first pass found under
/// it. A type that refuses even that refuses the probe, and the error says so.
pub fn keys_of<T: DeserializeOwned>() -> Result<Keys, Error> {
    let state = State::default();
    loop {
        state.failed.borrow_mut().take();
        let pass = T::deserialize(Probe {
            at:   String::new(),
            keys: &state,
        });
        match pass {
            Ok(_) => return Ok(state.keys.into_inner()),
            Err(e) => {
                let Some(at) = state.failed.borrow_mut().take() else { return Err(e) };
                if !state.refused.borrow_mut().insert(at) {
                    return Err(e);
                }
            },
        }
    }
}

/// What the passes of one probe share.
#[derive(Default)]
struct State {
    keys:    RefCell<Keys>,
    /// The keys whose value a check refused on an earlier pass, offered empty.
    refused: RefCell<BTreeSet<String>>,
    /// The innermost key whose value was refused on this pass.
    failed:  RefCell<Option<String>>,
}

struct Probe<'k> {
    at:   String,
    keys: &'k State,
}

impl<'k> Probe<'k> {
    fn under(&self, key: &str) -> Probe<'k> {
        Probe {
            at:   if self.at.is_empty() { key.to_string() } else { format!("{}.{key}", self.at) },
            keys: self.keys,
        }
    }

    fn same(&self) -> Probe<'k> {
        Probe {
            at:   self.at.clone(),
            keys: self.keys,
        }
    }

    /// Whether a check refused this key's value on an earlier pass.
    fn refused(&self) -> bool {
        self.keys.refused.borrow().contains(&self.at)
    }

    fn leaf(&self) {
        if !self.at.is_empty() {
            self.keys.keys.borrow_mut().read.insert(self.at.clone());
        }
    }

    fn opaque(&self) {
        self.leaf();
        if !self.at.is_empty() {
            self.keys.keys.borrow_mut().opaque.insert(self.at.clone());
        }
    }
}

macro_rules! leaf {
    ($($method:ident => $visit:ident($($value:expr)?)),* $(,)?) => {
        $(fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            self.leaf();
            visitor.$visit($($value)?)
        })*
    };
}

impl<'de> de::Deserializer<'de> for Probe<'_> {
    type Error = Error;

    leaf! {
        deserialize_bool => visit_bool(false),
        deserialize_i8 => visit_i64(1),
        deserialize_i16 => visit_i64(1),
        deserialize_i32 => visit_i64(1),
        deserialize_i64 => visit_i64(1),
        deserialize_i128 => visit_i128(1),
        deserialize_u8 => visit_u64(1),
        deserialize_u16 => visit_u64(1),
        deserialize_u32 => visit_u64(1),
        deserialize_u64 => visit_u64(1),
        deserialize_u128 => visit_u128(1),
        deserialize_f32 => visit_f64(1.0),
        deserialize_f64 => visit_f64(1.0),
        deserialize_char => visit_char('x'),
        deserialize_str => visit_str("x"),
        deserialize_string => visit_str("x"),
        deserialize_bytes => visit_bytes(b"x"),
        deserialize_byte_buf => visit_bytes(b"x"),
        deserialize_unit => visit_unit(),
        deserialize_identifier => visit_str("x"),
        deserialize_ignored_any => visit_unit(),
    }

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.opaque();
        visitor.visit_str("x")
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if self.refused() {
            return visitor.visit_none();
        }
        visitor.visit_some(self)
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.leaf();
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let left = usize::from(!self.refused());
        visitor.visit_seq(Elements {
            probe: self,
            left,
        })
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_seq(Elements {
            probe: self,
            left:  len,
        })
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let fields: &'static [&'static str] = if self.refused() { &[] } else { &["*"] };
        visitor.visit_map(Fields {
            probe: self,
            fields,
            next: None,
        })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        if fields.is_empty() {
            self.leaf();
        }
        visitor.visit_map(Fields {
            probe: self,
            fields,
            next: None,
        })
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.leaf();
        let first = variants
            .first()
            .ok_or_else(|| de::Error::custom("an enum with no variant reads no value"))?;
        visitor.visit_enum(Variant {
            probe: self,
            name:  first,
        })
    }
}

/// An array's elements, each under the array's own key.
struct Elements<'k> {
    probe: Probe<'k>,
    left:  usize,
}

impl<'de> SeqAccess<'de> for Elements<'_> {
    type Error = Error;

    fn next_element_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, Error> {
        if self.left == 0 {
            return Ok(None);
        }
        self.left -= 1;
        seed.deserialize(self.probe.same()).map(Some)
    }
}

/// A struct's fields, or a map's one key `*`, each with its value under it.
struct Fields<'k> {
    probe:  Probe<'k>,
    fields: &'static [&'static str],
    next:   Option<&'static str>,
}

impl<'de> MapAccess<'de> for Fields<'_> {
    type Error = Error;

    fn next_key_seed<S: DeserializeSeed<'de>>(
        &mut self,
        seed: S,
    ) -> Result<Option<S::Value>, Error> {
        let Some((first, rest)) = self.fields.split_first() else { return Ok(None) };
        self.fields = rest;
        self.next = Some(first);
        let key: StrDeserializer<'_, Error> = first.into_deserializer();
        seed.deserialize(key).map(Some)
    }

    fn next_value_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<S::Value, Error> {
        let key = self
            .next
            .take()
            .ok_or_else(|| de::Error::custom("a value asked for before its key"))?;
        let probe = self.probe.under(key);
        let at = probe.at.clone();
        let state = probe.keys;
        seed.deserialize(probe).inspect_err(|_| {
            state.failed.borrow_mut().get_or_insert(at);
        })
    }
}

/// An enum's first variant, which is what the probe offers.
struct Variant<'k> {
    probe: Probe<'k>,
    name:  &'static str,
}

impl<'de, 'k> EnumAccess<'de> for Variant<'k> {
    type Error = Error;
    type Variant = Probe<'k>;

    fn variant_seed<S: DeserializeSeed<'de>>(
        self,
        seed: S,
    ) -> Result<(S::Value, Probe<'k>), Error> {
        let name: StrDeserializer<'_, Error> = self.name.into_deserializer();
        Ok((seed.deserialize(name)?, self.probe))
    }
}

impl<'de> VariantAccess<'de> for Probe<'_> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        Ok(())
    }

    fn newtype_variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, Error> {
        seed.deserialize(self)
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_tuple(self, len, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_map(Fields {
            probe: self,
            fields,
            next: None,
        })
    }
}
