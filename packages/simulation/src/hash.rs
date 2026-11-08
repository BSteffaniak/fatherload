//! Deterministic, allocation-free hashing of canonical simulation state.
//!
//! [`StateHasher`] is a 64-bit FNV-1a writer. Plain scalar fields are fed to it directly, and
//! composite values are fed through a minimal [`serde::Serializer`] implementation that writes
//! every primitive (plus enum variant indices, lengths, and option tags) straight into the hasher
//! without building any intermediate encoding. The resulting hash is only guaranteed to be stable
//! within one build of the game; it is not a persistent or cross-version format.

use std::fmt;

use serde::{Serialize, ser};

/// Hash of the canonical simulation state at one tick.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SimulationStateHash(u64);

impl SimulationStateHash {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Streaming FNV-1a hasher used to build a [`SimulationStateHash`].
#[derive(Clone, Copy, Debug)]
pub struct StateHasher(u64);

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StateHasher {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    // Tags keep differently shaped values from colliding (for example `None` vs `Some(0)`).
    const TAG_NONE: u8 = 0;
    const TAG_SOME: u8 = 1;
    const TAG_UNIT: u8 = 2;
    const TAG_SEQ_UNKNOWN_LEN: u8 = 3;
    const TAG_END: u8 = 4;

    #[must_use]
    pub const fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    #[must_use]
    pub const fn finish(self) -> SimulationStateHash {
        SimulationStateHash(self.0)
    }

    pub const fn write_u8(&mut self, value: u8) {
        self.0 ^= value as u64;
        self.0 = self.0.wrapping_mul(Self::PRIME);
    }

    pub const fn write_bytes(&mut self, bytes: &[u8]) {
        let mut index = 0;
        while index < bytes.len() {
            self.write_u8(bytes[index]);
            index += 1;
        }
    }

    pub const fn write_u64(&mut self, value: u64) {
        let bytes = value.to_le_bytes();
        self.write_bytes(&bytes);
    }

    pub fn write_usize(&mut self, value: usize) {
        self.write_u64(u64::try_from(value).unwrap_or(u64::MAX));
    }

    pub const fn write_i32(&mut self, value: i32) {
        self.write_u64(value.cast_unsigned() as u64);
    }

    pub const fn write_i64(&mut self, value: i64) {
        self.write_u64(value.cast_unsigned());
    }

    pub const fn write_f32(&mut self, value: f32) {
        self.write_u64(value.to_bits() as u64);
    }

    /// Feeds any serializable value into the hasher, field by field, without allocating.
    pub fn write_serialized<T: Serialize + ?Sized>(&mut self, value: &T) {
        // `StateHasher` never reports errors itself; only a `Serialize` implementation calling
        // `ser::Error::custom` could fail, and canonical simulation types never do.
        if value.serialize(&mut *self).is_err() {
            self.write_u8(Self::TAG_END);
            self.write_u64(u64::MAX);
        }
    }

    fn write_len(&mut self, len: Option<usize>) {
        match len {
            Some(len) => self.write_usize(len),
            None => self.write_u8(Self::TAG_SEQ_UNKNOWN_LEN),
        }
    }

    fn write_str_value(&mut self, value: &str) {
        self.write_usize(value.len());
        self.write_bytes(value.as_bytes());
    }
}

/// Error type required by [`serde::Serializer`]; hashing itself cannot fail.
#[derive(Debug)]
pub struct StateHashError;

impl fmt::Display for StateHashError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("simulation state value could not be hashed")
    }
}

impl std::error::Error for StateHashError {}

impl ser::Error for StateHashError {
    fn custom<T: fmt::Display>(_message: T) -> Self {
        Self
    }
}

impl ser::Serializer for &mut StateHasher {
    type Ok = ();
    type Error = StateHashError;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, value: bool) -> Result<(), StateHashError> {
        self.write_u8(u8::from(value));
        Ok(())
    }

    fn serialize_i8(self, value: i8) -> Result<(), StateHashError> {
        self.write_i64(i64::from(value));
        Ok(())
    }

    fn serialize_i16(self, value: i16) -> Result<(), StateHashError> {
        self.write_i64(i64::from(value));
        Ok(())
    }

    fn serialize_i32(self, value: i32) -> Result<(), StateHashError> {
        self.write_i64(i64::from(value));
        Ok(())
    }

    fn serialize_i64(self, value: i64) -> Result<(), StateHashError> {
        self.write_i64(value);
        Ok(())
    }

    fn serialize_i128(self, value: i128) -> Result<(), StateHashError> {
        self.write_bytes(&value.to_le_bytes());
        Ok(())
    }

    fn serialize_u8(self, value: u8) -> Result<(), StateHashError> {
        self.write_u64(u64::from(value));
        Ok(())
    }

    fn serialize_u16(self, value: u16) -> Result<(), StateHashError> {
        self.write_u64(u64::from(value));
        Ok(())
    }

    fn serialize_u32(self, value: u32) -> Result<(), StateHashError> {
        self.write_u64(u64::from(value));
        Ok(())
    }

    fn serialize_u64(self, value: u64) -> Result<(), StateHashError> {
        self.write_u64(value);
        Ok(())
    }

    fn serialize_u128(self, value: u128) -> Result<(), StateHashError> {
        self.write_bytes(&value.to_le_bytes());
        Ok(())
    }

    fn serialize_f32(self, value: f32) -> Result<(), StateHashError> {
        self.write_f32(value);
        Ok(())
    }

    fn serialize_f64(self, value: f64) -> Result<(), StateHashError> {
        self.write_u64(value.to_bits());
        Ok(())
    }

    fn serialize_char(self, value: char) -> Result<(), StateHashError> {
        self.write_u64(u64::from(u32::from(value)));
        Ok(())
    }

    fn serialize_str(self, value: &str) -> Result<(), StateHashError> {
        self.write_str_value(value);
        Ok(())
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<(), StateHashError> {
        self.write_usize(value.len());
        self.write_bytes(value);
        Ok(())
    }

    fn serialize_none(self) -> Result<(), StateHashError> {
        self.write_u8(StateHasher::TAG_NONE);
        Ok(())
    }

    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), StateHashError> {
        self.write_u8(StateHasher::TAG_SOME);
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), StateHashError> {
        self.write_u8(StateHasher::TAG_UNIT);
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), StateHashError> {
        self.write_u8(StateHasher::TAG_UNIT);
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<(), StateHashError> {
        self.write_u64(u64::from(variant_index));
        Ok(())
    }

    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<(), StateHashError> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<(), StateHashError> {
        self.write_u64(u64::from(variant_index));
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self, StateHashError> {
        self.write_len(len);
        Ok(self)
    }

    fn serialize_tuple(self, len: usize) -> Result<Self, StateHashError> {
        self.write_usize(len);
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self, StateHashError> {
        self.write_usize(len);
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self, StateHashError> {
        self.write_u64(u64::from(variant_index));
        self.write_usize(len);
        Ok(self)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self, StateHashError> {
        self.write_len(len);
        Ok(self)
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<Self, StateHashError> {
        self.write_usize(len);
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self, StateHashError> {
        self.write_u64(u64::from(variant_index));
        self.write_usize(len);
        Ok(self)
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

impl ser::SerializeSeq for &mut StateHasher {
    type Ok = ();
    type Error = StateHashError;

    fn serialize_element<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<(), StateHashError> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), StateHashError> {
        self.write_u8(StateHasher::TAG_END);
        Ok(())
    }
}

impl ser::SerializeTuple for &mut StateHasher {
    type Ok = ();
    type Error = StateHashError;

    fn serialize_element<T: Serialize + ?Sized>(
        &mut self,
        value: &T,
    ) -> Result<(), StateHashError> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), StateHashError> {
        Ok(())
    }
}

impl ser::SerializeTupleStruct for &mut StateHasher {
    type Ok = ();
    type Error = StateHashError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), StateHashError> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), StateHashError> {
        Ok(())
    }
}

impl ser::SerializeTupleVariant for &mut StateHasher {
    type Ok = ();
    type Error = StateHashError;

    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), StateHashError> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), StateHashError> {
        Ok(())
    }
}

impl ser::SerializeMap for &mut StateHasher {
    type Ok = ();
    type Error = StateHashError;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), StateHashError> {
        key.serialize(&mut **self)
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), StateHashError> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<(), StateHashError> {
        self.write_u8(StateHasher::TAG_END);
        Ok(())
    }
}

impl ser::SerializeStruct for &mut StateHasher {
    type Ok = ();
    type Error = StateHashError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<(), StateHashError> {
        value.serialize(&mut **self)
    }

    fn skip_field(&mut self, _key: &'static str) -> Result<(), StateHashError> {
        self.write_u8(StateHasher::TAG_NONE);
        Ok(())
    }

    fn end(self) -> Result<(), StateHashError> {
        Ok(())
    }
}

impl ser::SerializeStructVariant for &mut StateHasher {
    type Ok = ();
    type Error = StateHashError;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        _key: &'static str,
        value: &T,
    ) -> Result<(), StateHashError> {
        value.serialize(&mut **self)
    }

    fn skip_field(&mut self, _key: &'static str) -> Result<(), StateHashError> {
        self.write_u8(StateHasher::TAG_NONE);
        Ok(())
    }

    fn end(self) -> Result<(), StateHashError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::StateHasher;

    fn hash_of<T: serde::Serialize>(value: &T) -> u64 {
        let mut hasher = StateHasher::new();
        hasher.write_serialized(value);
        hasher.finish().get()
    }

    #[test]
    fn serialized_hash_is_deterministic_and_shape_sensitive() {
        assert_eq!(hash_of(&Some(0_u32)), hash_of(&Some(0_u32)));
        assert_ne!(hash_of(&Some(0_u32)), hash_of(&None::<u32>));
        assert_ne!(hash_of(&vec![1_u32, 2]), hash_of(&vec![2_u32, 1]));
        assert_ne!(
            hash_of(&(vec![1_u32], vec![2_u32])),
            hash_of(&(vec![1_u32, 2], Vec::<u32>::new()))
        );
        let map = BTreeMap::from([(1_u32, 2.5_f32), (3, 4.0)]);
        assert_eq!(hash_of(&map), hash_of(&map.clone()));
        assert_ne!(hash_of(&1.0_f32), hash_of(&-1.0_f32));
    }
}
