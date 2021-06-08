// MIT License
//
// Copyright (c) 2019-2021 Tobias Pfeiffer
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use {
	super::*,
	std::{str::Utf8Error, marker::PhantomData, cell::RefCell},
	serde::{forward_to_deserialize_any, de::*}
};

pub fn deserialize<T: DeserializeOwned, R: io::Read>(reader: &mut R) -> Result<T, Error> {
	let mut len = [0u8; 4];
	reader.read_exact(&mut len).map_err(Error::Io)?;
	let len = i32::from_le_bytes(len);

	let mut buf = Vec::new();
	buf.resize(len as _, 0);
	buf[..4].copy_from_slice(&len.to_le_bytes());
	reader.read_exact(&mut buf[4..]).map_err(Error::Io)?;

	T::deserialize(&mut Deserializer::new(&buf))
}

pub(super) fn read_str_nt(buf: &[u8]) -> Result<&str, std::str::Utf8Error> {
	let mut len = 0;
	while buf[len] != 0 { len += 1; }
	std::str::from_utf8(&buf[..len])
}

#[derive(Debug)]
pub enum Error {
	Io(std::io::Error),
	EndOfStream,
	InvalidState,
	InvalidBsonType(u8),
	InvalidUtf8(Utf8Error),
	F128Unsupported,
	Unknown(String),
	InvalidType(String),
	InvalidValue(String),
	InvalidLength(usize, String),
	UnknownVariant(String),
	UnknownField(String),
	MissingField(&'static str),
	DuplicatedField(&'static str)
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		std::fmt::Debug::fmt(self, f)
	}
}

impl std::error::Error for Error {}

impl serde::de::Error for Error {
	fn custom<T: std::fmt::Display>(msg: T) -> Self {
		Self::Unknown(msg.to_string())
	}
	
	fn invalid_type(_unexp: Unexpected, exp: &dyn Expected) -> Self {
		Self::InvalidType(exp.to_string())
	}
	
	fn invalid_value(_unexp: Unexpected, exp: &dyn Expected) -> Self {
		Self::InvalidValue(exp.to_string())
	}
	
	fn invalid_length(len: usize, exp: &dyn Expected) -> Self {
		Self::InvalidLength(len, exp.to_string())
	}
	
	fn unknown_variant(variant: &str, _expected: &'static [&'static str]) -> Self {
		Self::UnknownVariant(variant.to_string())
	}
	
	fn unknown_field(field: &str, _expected: &'static [&'static str]) -> Self {
		Self::UnknownField(field.to_string())
	}
	
	fn missing_field(field: &'static str) -> Self {
		Self::MissingField(field)
	}
	
	fn duplicate_field(field: &'static str) -> Self {
		Self::DuplicatedField(field)
	}
}

pub struct Deserializer<'de> {
	buf:  &'de [u8],
	next: Option<u8>
}

impl<'de> Deserializer<'de> {
	pub fn new(buf: &'de [u8]) -> Self {
		Self { buf, next: Some(Document::ID), }
	}
	
	fn deserialize<'a, T: 'de>(&'a mut self, state: isize) -> __RefCell__SpecialDeserializer__<'a, 'de, T> {
		__RefCell__SpecialDeserializer__(RefCell::new(SpecialDeserializer { inner: self, state, marker: PhantomData }))
	}
}

impl<'a, 'de> serde::Deserializer<'de> for &'a mut Deserializer<'de> {
	type Error = Error;
	
	fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		match self.next.ok_or(Error::InvalidState)? {
			bool::ID => {
				let v = self.buf[0] == 1;
				self.buf = &self.buf[1..];
				visitor.visit_bool(v)
			}
			i32::ID => {
				let v = i32::from_le_bytes([
					self.buf[0], self.buf[1], self.buf[2], self.buf[3]
				]);

				self.buf = &self.buf[4..];
				visitor.visit_i32(v)
			}
			i64::ID => {
				let v = i64::from_le_bytes([
					self.buf[0], self.buf[1], self.buf[2], self.buf[3],
					self.buf[4], self.buf[5], self.buf[6], self.buf[7]
				]);

				self.buf = &self.buf[8..];
				visitor.visit_i64(v)
			}
			f64::ID => {
				let v = f64::from_le_bytes([
					self.buf[0], self.buf[1], self.buf[2], self.buf[3],
					self.buf[4], self.buf[5], self.buf[6], self.buf[7]
				]);

				self.buf = &self.buf[8..];
				visitor.visit_f64(v)
			}
			f128::ID => Err(Error::F128Unsupported),
			<&str>::ID => {
				let len = i32::from_le_bytes([
					self.buf[0], self.buf[1], self.buf[2], self.buf[3]
				]) as usize;

				let str = std::str::from_utf8(&self.buf[4..4 + len - 1])
					.map_err(Error::InvalidUtf8)?;

				self.buf = &self.buf[5 + str.len()..];
				visitor.visit_borrowed_str(str)
			}
			<()>::ID => visitor.visit_unit(),
			0x03 => { self.buf = &self.buf[4..]; visitor.visit_map(self) }
			0x04 => { self.buf = &self.buf[4..]; visitor.visit_seq(self) }
			0x05 => visitor.visit_map(&self.deserialize::<BinaryData>(6)),
			0x07 => visitor.visit_map(&self.deserialize::<ObjectId>(2)),
			0x09 => visitor.visit_map(&self.deserialize::<UtcDateTime>( 2)),
			0x0B => visitor.visit_map(&self.deserialize::<Regex>( 6)),
			0x0D => visitor.visit_map(&self.deserialize::<JavaScript>( 2)),
			0x0F => visitor.visit_map(&self.deserialize::<JavaScriptWithScope>( 4)),
			0x11 => visitor.visit_map(&self.deserialize::<Timestamp>(2)),
			id => Err(Error::InvalidBsonType(id))
		}
	}
	
	fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		match self.next {
			Some(<&str>::ID) => { self.next.take(); },
			_ => return self.deserialize_any(visitor)
		}

		let len = i32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;

		let str = std::str::from_utf8(&self.buf[4..4 + len - 1])
			.map_err(Error::InvalidUtf8)?;

		self.buf = &self.buf[5 + str.len()..];
		visitor.visit_char(str.chars().next().ok_or_else(
			|| Error::InvalidLength(str.len(), "failed to deserialize char".to_string()))?)
	}
	
	fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		match self.next {
			Some(<&[u8]>::ID) => {
				self.next.take();

				let bytes = &self.buf[5..5 + i32::from_le_bytes([
					self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize];

				self.buf = &self.buf[5 + bytes.len()..];
				visitor.visit_borrowed_bytes(bytes)
			}
			Some(Document::ID) => {
				self.next.take();

				let bytes = &self.buf[..i32::from_le_bytes(
					[self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize];

				self.buf = &self.buf[bytes.len()..];
				visitor.visit_borrowed_bytes(bytes)
			},
			_ => self.deserialize_any(visitor)
		}
	}
	
	fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		self.deserialize_bytes(visitor)
	}
	
	fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		match self.next {
			Some(<()>::ID) => visitor.visit_none(),
			_ => visitor.visit_some(self)
		}
	}
	
	fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		visitor.visit_newtype_struct(self)
	}
	
	fn deserialize_enum<V>(self, _name: &'static str, _variants: &'static [&'static str], visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		if self.next.ok_or(Error::InvalidState)? == 0x03 {
			self.buf = &self.buf[4..];
			visitor.visit_enum(self)
		} else {
			Err(Error::InvalidBsonType(self.next.unwrap()))
		}
	}
	
	fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		Err(Error::InvalidState)
	}
	
	fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		let id = self.next.take().ok_or(Error::InvalidState)?;
		self.buf = &self.buf[get_value_size(id, self.buf)..];
		visitor.visit_unit()
	}
	
	fn is_human_readable(&self) -> bool {
		false
	}
	
	forward_to_deserialize_any! {
		bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 str string unit unit_struct seq tuple
		tuple_struct map struct
	}
}

impl<'de> MapAccess<'de> for Deserializer<'de> {
	type Error = Error;
	
	fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error> where
		K: DeserializeSeed<'de> {
		if self.buf[0] == 0 { self.buf = &self.buf[1..]; return Ok(None) }
		self.next = Some(self.buf[0]);
		self.buf = &self.buf[1..];
		seed.deserialize(IdDeserializer(self)).map(Some)
	}
	
	fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error> where
		V: DeserializeSeed<'de> {
		seed.deserialize(self)
	}
}

impl<'de> SeqAccess<'de> for Deserializer<'de> {
	type Error = Error;
	
	fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error> where
		T: DeserializeSeed<'de> {
		if self.buf[0] == 0 { self.buf = &self.buf[1..]; return Ok(None) }
		self.next = Some(self.buf[0]);
		while self.buf[0] != 0 { self.buf = &self.buf[1..] }
		self.buf = &self.buf[1..];
		seed.deserialize(self).map(Some)
	}
}

impl<'a, 'de> EnumAccess<'de> for &'a mut Deserializer<'de> {
	type Error   = Error;
	type Variant = Self;

	fn variant_seed<V>(self, seed: V) -> Result<(<V as DeserializeSeed<'de>>::Value, Self::Variant), Self::Error> where
		V: DeserializeSeed<'de> {
		if self.buf[0] == 0 { self.buf = &self.buf[1..]; return Err(Error::EndOfStream) }
		self.next = Some(self.buf[0]);
		self.buf = &self.buf[1..];
		seed.deserialize(IdDeserializer(&mut*self)).map(move |v| (v, self))
	}
}

impl<'a, 'de> VariantAccess<'de> for &'a mut Deserializer<'de> {
	type Error = Error;

	fn unit_variant(self) -> Result<(), Self::Error> {
		Ok(())
	}

	fn newtype_variant_seed<T>(self, seed: T) -> Result<<T as DeserializeSeed<'de>>::Value, Self::Error> where
		T: DeserializeSeed<'de> {
		seed.deserialize(self)
	}

	fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<<V as Visitor<'de>>::Value, Self::Error> where
		V: Visitor<'de> {
		serde::de::Deserializer::deserialize_seq(self, visitor)
	}

	fn struct_variant<V>(self, _fields: &'static [&'static str], visitor: V) -> Result<<V as Visitor<'de>>::Value, Self::Error> where
		V: Visitor<'de> {
		serde::de::Deserializer::deserialize_map(self, visitor)
	}
}

pub struct IdDeserializer<'a, 'de>(&'a mut Deserializer<'de>);

impl<'a, 'de> serde::Deserializer<'de> for IdDeserializer<'a, 'de> {
	type Error = Error;
	
	fn deserialize_any<V>(self, visitor: V) -> Result<<V as Visitor<'de>>::Value, Self::Error> where
		V: Visitor<'de> {
		let str = read_str_nt(self.0.buf).map_err(Error::InvalidUtf8)?;
		self.0.buf = &self.0.buf[str.len() + 1..];
		visitor.visit_borrowed_str(str)
	}
	
	forward_to_deserialize_any! {
		bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
		option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum
		ignored_any identifier
	}
}

struct SpecialDeserializer<'a, 'de: 'a, T: 'de> {
	inner:  &'a mut Deserializer<'de>,
	state:  isize,
	marker: PhantomData<T>
}

/// This hack is required due to lifetime errors (┛ಠ_ಠ)┛彡┻━┻
#[allow(non_camel_case_types)]
struct __RefCell__SpecialDeserializer__<'a, 'de, T: 'de>(RefCell<SpecialDeserializer<'a, 'de, T>>);

impl<'a, 'b, 'de, T: 'de> MapAccess<'de> for &'a __RefCell__SpecialDeserializer__<'b, 'de, T>
	where Self: serde::Deserializer<'de, Error = Error>
{
	type Error = Error;
	
	fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error> where
		K: DeserializeSeed<'de> {
		let state = self.0.borrow().state; // ref has to be dropped here
		match state {
			0 => Ok(None),
			_ => seed.deserialize(&**self).map(Some)
		}
	}
	
	fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error> where
		V: DeserializeSeed<'de> {
		seed.deserialize(&**self)
	}
}

impl<'a, 'b, 'de> serde::Deserializer<'de> for &'a __RefCell__SpecialDeserializer__<'b, 'de, BinaryData<'de>> {
	type Error = Error;
	
	fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		let mut self_ = self.0.borrow_mut();
		self_.state -= 1;
		match self_.state {
			5 => visitor.visit_borrowed_str("$binary"),
			4 => visitor.visit_map(self),
			3 => visitor.visit_borrowed_str("type"),
			2 => visitor.visit_u8(self_.inner.buf[4]),
			1 => visitor.visit_borrowed_str("base64"),
			0 => {
				let v = &self_.inner.buf[5..5 + i32::from_le_bytes([
					self_.inner.buf[0], self_.inner.buf[1], self_.inner.buf[2], self_.inner.buf[3]]) as usize];
				self_.inner.buf = &self_.inner.buf[v.len() + 5..];
				visitor.visit_borrowed_bytes(v)
			}
			_ => unreachable!()
		}
	}
	
	forward_to_deserialize_any! {
		bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
		option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum
		ignored_any identifier
	}
}

impl<'a, 'de> serde::Deserializer<'de> for &'a __RefCell__SpecialDeserializer__<'a, 'de, ObjectId> {
	type Error = Error;
	
	fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		let mut self_ = self.0.borrow_mut();
		self_.state -= 1;
		match self_.state {
			1 => visitor.visit_borrowed_str("$oid"),
			0 => {
				let v = visitor.visit_borrowed_bytes(&self_.inner.buf[..12]);
				self_.inner.buf = &self_.inner.buf[12..];
				v
			}
			_ => unreachable!()
		}
	}
	
	forward_to_deserialize_any! {
		bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
		option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum
		ignored_any identifier
	}
}

impl<'a, 'de> serde::Deserializer<'de> for &'a __RefCell__SpecialDeserializer__<'a, 'de, UtcDateTime> {
	type Error = Error;
	
	fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		let mut self_ = self.0.borrow_mut();
		self_.state -= 1;
		match self_.state {
			1 => visitor.visit_borrowed_str("$date"),
			0 => {
				let r = visitor.visit_i64(i64::from_le_bytes([
					self_.inner.buf[0], self_.inner.buf[1], self_.inner.buf[2], self_.inner.buf[3],
					self_.inner.buf[4], self_.inner.buf[5], self_.inner.buf[6], self_.inner.buf[7],
				]));

				self_.inner.buf = &self_.inner.buf[8..];
				r
			},
			_ => Err(Error::InvalidState)
		}
	}
	
	forward_to_deserialize_any! {
		bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
		option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum
		ignored_any identifier
	}
}

impl<'a, 'de> serde::Deserializer<'de> for &'a __RefCell__SpecialDeserializer__<'a, 'de, Regex<'de>> {
	type Error = Error;
	
	fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		let mut self_ = self.0.borrow_mut();
		self_.state -= 1;
		match self_.state {
			5 => visitor.visit_borrowed_str("$regularExpression"),
			//4 => visitor.visit_map(self_),
			3 => visitor.visit_borrowed_str("pattern"),
			1 => visitor.visit_borrowed_str("options"),
			2 | 0 =>  read_str_nt(self_.inner.buf)
				.map(|s| { self_.inner.buf = &self_.inner.buf[s.len() + 1..]; s })
				.map_err(Error::InvalidUtf8)
				.and_then(|v| visitor.visit_borrowed_str(v)),
			_ => Err(Error::InvalidState)
		}
	}
	
	forward_to_deserialize_any! {
		bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
		option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum
		ignored_any identifier
	}
}

impl<'a, 'de> serde::Deserializer<'de> for &'a __RefCell__SpecialDeserializer__<'a, 'de, JavaScript<'de>> {
	type Error = Error;
	
	fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		let mut self_ = self.0.borrow_mut();
		self_.state -= 1;
		match self_.state {
			1 => visitor.visit_borrowed_str("$code"),
			0 => std::str::from_utf8(&self_.inner.buf[4..4 + i32::from_le_bytes([
				self_.inner.buf[0], self_.inner.buf[1], self_.inner.buf[2], self_.inner.buf[3]]) as usize - 1])
				.map(|s| { self_.inner.buf = &self_.inner.buf[4 + s.len()..]; s })
				.map_err(Error::InvalidUtf8)
				.and_then(|v| visitor.visit_borrowed_str(v)),
			_ => Err(Error::InvalidState)
		}
	}
	
	forward_to_deserialize_any! {
		bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
		option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum
		ignored_any identifier
	}
}

impl<'a, 'de> serde::Deserializer<'de> for &'a __RefCell__SpecialDeserializer__<'a, 'de, JavaScriptWithScope<'de>> {
	type Error = Error;
	
	fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		let mut self_ = self.0.borrow_mut();
		self_.state -= 1;
		match self_.state {
			3 => visitor.visit_borrowed_str("$code"),
			2 => std::str::from_utf8(&self_.inner.buf[4..4 + i32::from_le_bytes([
				self_.inner.buf[0], self_.inner.buf[1], self_.inner.buf[2], self_.inner.buf[3]]) as usize - 1])
				.map(|s| { self_.inner.buf = &self_.inner.buf[4 + s.len()..]; s })
				.map_err(Error::InvalidUtf8)
				.and_then(|v| visitor.visit_borrowed_str(v)),
			1 => visitor.visit_borrowed_str("$scope"),
			0 => {
				let v = &self_.inner.buf[4..i32::from_le_bytes([
					self_.inner.buf[0], self_.inner.buf[1], self_.inner.buf[2], self_.inner.buf[3]]) as usize];

				self_.inner.buf = &self_.inner.buf[4 + v.len()..];
				visitor.visit_borrowed_bytes(v)
			}
			_ => Err(Error::InvalidState)
		}
	}
	
	forward_to_deserialize_any! {
		bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
		option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum
		ignored_any identifier
	}
}

impl<'a, 'de> serde::Deserializer<'de> for &'a __RefCell__SpecialDeserializer__<'a, 'de, Timestamp> {
	type Error = Error;
	
	fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error> where
		V: Visitor<'de> {
		let mut self_ = self.0.borrow_mut();
		self_.state -= 1;
		match self_.state {
			1 => visitor.visit_borrowed_str("$timestamp"),
			0 => visitor.visit_u64({
				let v = u64::from_le_bytes([
					self_.inner.buf[0], self_.inner.buf[1], self_.inner.buf[2], self_.inner.buf[3],
					self_.inner.buf[4], self_.inner.buf[5], self_.inner.buf[6], self_.inner.buf[7]
				]);

				self_.inner.buf = &self_.inner.buf[8..];
				v
			}),
			_ => Err(Error::InvalidState)
		}
	}
	
	forward_to_deserialize_any! {
		bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
		option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum
		ignored_any identifier
	}
}
