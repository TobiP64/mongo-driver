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

#![allow(clippy::zero_prefixed_literal)]

use {
	std::{io, mem::size_of},
	serde::{Serialize, Deserialize, Serializer, Deserializer}
};

pub use self::{
	doc::*,
	impls::*,
	binary_data::*,
	oid::*,
	utc_date_time::*,
	regex::*,
	script::*,
	timestamp::*,
	se::serialize,
	de::deserialize
};

pub mod doc;
pub mod se;
pub mod de;
pub mod oid;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Bson<'a> {
	Unit,
	Bool(bool),
	I32(i32),
	I64(i64),
	F64(f64),
	F128(f128),
	ObjectId(ObjectId),
	Timestamp(Timestamp),
	Document(DocRef<'a>),
	Array(ArrayRef<'a>),
	String(&'a str),
	Binary(BinaryData<'a>),
	Datetime(UtcDateTime),
	Regex(Regex<'a>),
	JavaScript(JavaScript<'a>),
	JavaScriptWithScope(JavaScriptWithScope<'a>)
}

impl<'a> Bson<'a> {
	/// Returns the id of the contained value.
	pub fn id(&self) -> u8 {
		match self {
			Bson::Unit => 0,
			Bson::Bool(_)                => bool::ID,
			Bson::I32(_)                 => i32::ID,
			Bson::I64(_)                 => i64::ID,
			Bson::F64(_)                 => f64::ID,
			Bson::F128(_)                => f128::ID,
			Bson::ObjectId(_)            => ObjectId::ID,
			Bson::Timestamp(_)           => Timestamp::ID,
			Bson::Document(_)            => DocRef::ID,
			Bson::Array(_)               => ArrayRef::ID,
			Bson::String(_)              => <&str>::ID,
			Bson::Binary(_)              => BinaryData::ID,
			Bson::Datetime(_)            => UtcDateTime::ID,
			Bson::Regex(_)               => Regex::ID,
			Bson::JavaScript(_)          => JavaScript::ID,
			Bson::JavaScriptWithScope(_) => JavaScriptWithScope::ID
		}
	}

	/// Returns the size of the contained value
	pub fn size(&self) -> usize {
		match self {
			Bson::Unit => 0,
			Bson::Bool(v) => v.size(),
			Bson::I32(v) => v.size(),
			Bson::I64(v) => v.size(),
			Bson::F64(v) => v.size(),
			Bson::F128(v) => v.size(),
			Bson::ObjectId(v) => v.size(),
			Bson::Timestamp(v) => v.size(),
			Bson::Document(v) => v.size(),
			Bson::Array(v) => v.size(),
			Bson::String(v) => v.size(),
			Bson::Binary(v) => v.size(),
			Bson::Datetime(v) => v.size(),
			Bson::Regex(v) => v.size(),
			Bson::JavaScript(v) => v.size(),
			Bson::JavaScriptWithScope(v) => v.size()
		}
	}

	/// Writes the contained value.
	pub fn write(&self, buf: &mut [u8]) {
		match self {
			Bson::Unit => (),
			Bson::Bool(v) => v.write_bson(buf),
			Bson::I32(v) => v.write_bson(buf),
			Bson::I64(v) => v.write_bson(buf),
			Bson::F64(v) => v.write_bson(buf),
			Bson::F128(v) => v.write_bson(buf),
			Bson::ObjectId(v) => v.write_bson(buf),
			Bson::Timestamp(v) => v.write_bson(buf),
			Bson::Document(v) => v.write_bson(buf),
			Bson::Array(v) => v.write_bson(buf),
			Bson::String(v) => v.write_bson(buf),
			Bson::Binary(v) => v.write_bson(buf),
			Bson::Datetime(v) => v.write_bson(buf),
			Bson::Regex(v) => v.write_bson(buf),
			Bson::JavaScript(v) => v.write_bson(buf),
			Bson::JavaScriptWithScope(v) => v.write_bson(buf)
		}
	}

	/// Invokes `BsonValue::read_bson` for the type with the given id.
	#[allow(clippy::result_unit_err)]
	pub fn read(id: u8, buf: &'a [u8]) -> Result<Self, ()> {
		Ok(match id {
			f64::ID                 => Bson::F64(f64::read_bson(buf)),
			<&str>::ID              => Bson::String(<&str>::read_bson(buf)),
			DocRef::ID              => Bson::Document(DocRef(&buf[4..])),
			ArrayRef::ID            => Bson::Array(ArrayRef(&buf[4..])),
			BinaryData::ID          => Bson::Binary(BinaryData::read_bson(buf)),
			ObjectId::ID            => Bson::ObjectId(ObjectId::read_bson(buf)),
			bool::ID                => Bson::Bool(bool::read_bson(buf)),
			UtcDateTime::ID         => Bson::Datetime(UtcDateTime::read_bson(buf)),
			<()>::ID                => Bson::Unit,
			Regex::ID               => Bson::Regex(Regex::read_bson(buf)),
			JavaScript::ID          => Bson::JavaScript(JavaScript::read_bson(buf)),
			JavaScriptWithScope::ID => Bson::JavaScriptWithScope(JavaScriptWithScope::read_bson(buf)),
			i32::ID                 => Bson::I32(i32::read_bson(buf)),
			Timestamp::ID           => Bson::Timestamp(Timestamp::read_bson(buf)),
			i64::ID                 => Bson::I64(i64::read_bson(buf)),
			f128::ID                => Bson::F128(f128::read_bson(buf)),
			_                       => return Err(())
		})
	}
}

impl<'a> Serialize for Bson<'a> {
	fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error> where
		S: Serializer {
		match self {
			Bson::Unit => serializer.serialize_unit(),
			Bson::Bool(v) => serializer.serialize_bool(*v),
			Bson::I32(v) => serializer.serialize_i32(*v),
			Bson::I64(v) => serializer.serialize_i64(*v),
			Bson::F64(v) => serializer.serialize_f64(*v),
			Bson::F128(_) => serializer.serialize_unit(),
			Bson::ObjectId(v) => v.serialize(serializer),
			Bson::Timestamp(v) => v.serialize(serializer),
			Bson::Document(v) => v.serialize(serializer),
			Bson::Array(v) => v.serialize(serializer),
			Bson::String(v) => v.serialize(serializer),
			Bson::Binary(v) => v.serialize(serializer),
			Bson::Datetime(v) => v.serialize(serializer),
			Bson::Regex(v) => v.serialize(serializer),
			Bson::JavaScript(v) => v.serialize(serializer),
			Bson::JavaScriptWithScope(v) => v.serialize(serializer),
		}
	}
}

impl From<()> for Bson<'_> { fn from(_: ()) -> Self { Bson::Unit } }
impl From<bool> for Bson<'_> { fn from(v: bool) -> Self { Bson::Bool(v) } }
impl From<i32> for Bson<'_> { fn from(v: i32) -> Self { Bson::I32(v) } }
impl From<i64> for Bson<'_> { fn from(v: i64) -> Self { Bson::I64(v) } }
impl From<f64> for Bson<'_> { fn from(v: f64) -> Self { Bson::F64(v) } }
impl From<f128> for Bson<'_> { fn from(v: f128) -> Self { Bson::F128(v) } }
impl From<ObjectId> for Bson<'_> { fn from(v: ObjectId) -> Self { Bson::ObjectId(v) } }
impl From<Timestamp> for Bson<'_> { fn from(v: Timestamp) -> Self { Bson::Timestamp(v) } }
impl<'a> From<DocRef<'a>> for Bson<'a> { fn from(v: DocRef<'a>) -> Self { Bson::Document(v) } }
impl<'a> From<ArrayRef<'a>> for Bson<'a> { fn from(v: ArrayRef<'a>) -> Self { Bson::Array(v) } }
impl<'a> From<&'a str> for Bson<'a> { fn from(v: &'a str) -> Self { Bson::String(v) } }
impl<'a> From<BinaryData<'a>> for Bson<'a> { fn from(v: BinaryData<'a>) -> Self { Bson::Binary(v) } }
impl From<UtcDateTime> for Bson<'_> { fn from(v: UtcDateTime) -> Self { Bson::Datetime(v) } }
impl<'a> From<Regex<'a>> for Bson<'a> { fn from(v: Regex<'a>) -> Self { Bson::Regex(v) } }
impl<'a> From<JavaScript<'a>> for Bson<'a> { fn from(v: JavaScript<'a>) -> Self { Bson::JavaScript(v) } }
impl<'a> From<JavaScriptWithScope<'a>> for Bson<'a> { fn from(v: JavaScriptWithScope<'a>) -> Self { Bson::JavaScriptWithScope(v) } }

/// A trait for reading and writing bson values with a constant id.
pub trait BsonValue<'a>: Sized {
	const ID: u8;
	
	fn size(&self) -> usize { size_of::<Self>() }

	/// Write self into the given buffer.
	fn write_bson(&self, buf: &mut [u8]);

	/// Read self from the given buffer.
	fn read_bson(buf: &'a [u8]) -> Self;
}

/// A trait for reading and writing bson values with a constant id.
pub trait OwnedBsonValue: Sized {
	const ID: u8;
	
	fn size(&self) -> usize { size_of::<Self>() }
	
	/// Write self into the given buffer.
	fn write_bson(&self, buf: &mut [u8]);
	
	/// Read self from the given buffer.
	fn read_bson(buf: &[u8]) -> Self;
}

/*impl<'a, T: Borrow<U>, U: BsonValue<'a>> OwnedBsonValue for T {
	const ID: u8 = U::ID;
	
	fn write_bson(&self, buf: &mut [u8]) {
		self.borrow().write_bson(buf)
	}
	
	fn read_bson(buf: &[u8]) -> Self {
		U::read_bson(buf).to_owned()
	}
}*/

/// Returns the size of the bson type with the given id by looking into the given buffer.
fn get_value_size(id: u8, buf: &[u8]) -> usize {
	match id {
		<()>::ID       => 0,
		bool::ID       => 1,
		i32::ID        => 4,
		ObjectId::ID   => 12,
		f128::ID       => 16,
		BinaryData::ID => 5 + i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize,
		Regex::ID      => {
			let mut len = 0;
			while buf[len] != 0 { len += 1; }
			len += 1;
			while buf[len] != 0 { len += 1; }
			len
		},
		UtcDateTime::ID | Timestamp::ID | i64::ID | f64::ID => 8,
		<&str>::ID | JavaScript::ID => 4 + i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize,
		 0x3 | 0x4 | JavaScriptWithScope::ID => i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize,
		_ => panic!("invalid bson")
	}
}

mod impls {
	use super::*;
	
	macro_rules! bson_impl {
	    ( $ty:ty, $ty1:ty ) => {
			impl BsonValue<'_> for $ty {
				const ID: u8 = <$ty1>::ID;
				
				fn size(&self) -> usize {
					<$ty1>::size(&(*self as _))
				}
				
				fn write_bson(&self, buf: &mut [u8]) {
					<$ty1>::write_bson(&(*self as _), buf)
				}
				
				fn read_bson(buf: &[u8]) -> Self {
					<$ty1>::read_bson(buf) as _
				}
			}
	    };
	}
	
	impl BsonValue<'_> for Document {
		const ID: u8 = 0x03;
		
		fn size(&self) -> usize {
			self.0.len()
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..self.size()].copy_from_slice(self.0.as_slice());
		}
		
		fn read_bson(buf: &[u8]) -> Self {
			Self(buf[..i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize].to_vec())
		}
	}
	
	impl<'a> BsonValue<'a> for DocRef<'a> {
		const ID: u8 = 0x03;
		
		fn size(&self) -> usize {
			self.0.len() + 4
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..4].copy_from_slice(&(self.size() as i32).to_le_bytes());
			buf[4..self.size()].copy_from_slice(self.0);
		}
		
		fn read_bson(buf: &'a [u8]) -> Self where {
			Self(&buf[4..i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize])
		}
	}
	
	impl BsonValue<'_> for Array {
		const ID: u8 = 0x04;
		
		fn size(&self) -> usize {
			self.0.len()
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..self.size()].copy_from_slice(self.0.as_slice());
		}
		
		fn read_bson(buf: &[u8]) -> Self {
			Self(buf[..i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize].to_vec())
		}
	}
	
	impl<'a> BsonValue<'a> for ArrayRef<'a> {
		const ID: u8 = 0x04;
		
		fn size(&self) -> usize {
			self.0.len() + 4
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..4].copy_from_slice(&(self.size() as i32).to_le_bytes());
			buf[4..self.size()].copy_from_slice(self.0);
		}
		
		fn read_bson(buf: &'a [u8]) -> Self {
			Self(&buf[4..i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize])
		}
	}
	
	impl BsonValue<'_> for () {
		const ID: u8 = 0x0A;
		
		fn write_bson(&self, _buf: &mut [u8]) {}
		
		fn read_bson(_buf: &[u8]) -> Self {}
	}
	
	impl BsonValue<'_> for bool {
		const ID: u8 = 0x08;
		
		fn size(&self) -> usize {
			1
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[0] = if *self { 1 } else { 0 };
		}
		
		fn read_bson(buf: &[u8]) -> Self {
			buf[0] != 0
		}
	}
	
	bson_impl!(i8,  i32);
	bson_impl!(i16, i32);
	bson_impl!(u8,  i32);
	bson_impl!(u16, i32);
	bson_impl!(u32, i32);
	bson_impl!(u64, i64);
	
	impl BsonValue<'_> for i32 {
		const ID: u8 = 0x10;
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..4].copy_from_slice(&self.to_le_bytes())
		}
		
		fn read_bson(buf: &[u8]) -> Self {
			Self::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
		}
	}
	
	impl BsonValue<'_> for i64 {
		const ID: u8 = 0x12;
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..8].copy_from_slice(&self.to_le_bytes())
		}
		
		fn read_bson(buf: &[u8]) -> Self {
			Self::from_le_bytes([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]])
		}
	}
	
	bson_impl!(f32, f64);
	
	impl BsonValue<'_> for f64 {
		const ID: u8 = 0x01;

		fn write_bson(&self, buf: &mut [u8]) {
			buf[..8].copy_from_slice(&self.to_le_bytes())
		}

		fn read_bson(buf: &[u8]) -> Self {
			Self::from_le_bytes([buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]])
		}
	}
	
	impl BsonValue<'_> for f128 {
		const ID: u8 = 0x13;

		fn write_bson(&self, buf: &mut [u8]) {
			buf[..16].copy_from_slice(&self.0.to_le_bytes());
		}

		fn read_bson(buf: &[u8]) -> Self {
			Self(u128::from_le_bytes([
				buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
				buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15]
			]))
		}
	}
	
	impl<'a> BsonValue<'a> for &'a [u8] {
		const ID: u8 = 0x05;
		
		fn size(&self) -> usize {
			5 + self.len()
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..4].copy_from_slice(&(self.len() as i32).to_le_bytes());
			buf[4] = BinarySubtype::Generic as _;
			buf[5..5 + self.len()].copy_from_slice(self);
		}
		
		fn read_bson(buf: &'a [u8]) -> Self {
			&buf[5..5 + i32::from_le_bytes(
				[buf[0], buf[1], buf[2], buf[3]]) as usize]
		}
	}
	
	impl<'a> BsonValue<'a> for &'a str {
		const ID: u8 = 0x02;
		
		fn size(&self) -> usize {
			5 + self.len()
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..4].copy_from_slice(&(self.len() as i32 + 1).to_le_bytes());
			buf[4..4 + self.len()].copy_from_slice(self.as_bytes());
			buf[4 + self.len()] = 0;
		}
		
		fn read_bson(buf: &'a [u8]) -> Self {
			std::str::from_utf8(&buf[4..4 + i32::from_le_bytes(
				[buf[0], buf[1], buf[2], buf[3]]) as usize - 1]).unwrap()
		}
	}
	
	#[repr(transparent)]
	#[allow(non_camel_case_types)]
	#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd, Hash)]
	pub struct f128(u128);
	
	//
	// ARRAYS
	//
	
	impl<T> BsonValue<'_> for [T; 0] {
		const ID: u8 = 0x4;
		
		fn size(&self) -> usize {
			5
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..4].copy_from_slice(&5i32.to_le_bytes());
			buf[4] = 0;
		}
		
		fn read_bson(_buf: &[u8]) -> Self {
			panic!()
		}
	}
	
	impl<'a> BsonValue<'a> for &'a [Bson<'a>] {
		const ID: u8 = 0x4;
		
		fn size(&self) -> usize {
			5 + self.iter().enumerate()
				.map(|(i, e)| 2 + i.to_string().len() + e.size())
				.sum::<usize>()
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			let mut off = 4;
			for (i, e) in self.iter().enumerate() {
				buf[off] = e.id();
				off += 1;

				let key = i.to_string();
				buf[off..off + key.len()].copy_from_slice(key.as_bytes());
				off += key.len();
				buf[off] = 0;

				e.write(&mut buf[off + 1..]);
				off += 1 + e.size();
			}
			buf[off] = 0;
			buf[..4].copy_from_slice(&(off as u32 + 1u32).to_le_bytes());
		}
		
		fn read_bson(_buf: &'a [u8]) -> Self {
			unimplemented!()
		}
	}
}

mod binary_data {
	use super::*;

	/// Binary data, a densely stored byte array with a type.
	#[derive(Debug, Copy, Clone, Eq, PartialEq)]
	pub struct BinaryData<'a> {
		pub binary: &'a [u8],
		pub r#type: BinarySubtype
	}
	
	impl<'a> BsonValue<'a> for BinaryData<'a> {
		const ID: u8 = 0x05;
		
		fn size(&self) -> usize {
			5 + self.binary.len()
		}
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf[..4].copy_from_slice(&(self.binary.len() as i32).to_le_bytes());
			buf[4] = self.r#type as _;
			buf[5..5 + self.binary.len()].copy_from_slice(self.binary);
		}
		
		fn read_bson(buf: &'a [u8]) -> Self {
			Self {
				binary: &buf[5..5 + i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize],
				r#type: buf[4].into()
			}
		}
	}
	
	impl Default for BinarySubtype {
		fn default() -> Self {
			BinarySubtype::Generic
		}
	}
	
	#[repr(u8)]
	#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
	pub enum BinarySubtype {
		Generic     = 0x0,
		Function    = 0x1,
		UUID        = 0x4,
		MD5         = 0x5,
		Encrypted   = 0x6,
		UserDefined = 0x80
	}
	
	impl From<u8> for BinarySubtype {
		fn from(v: u8) -> Self {
			match v {
				0 => Self::Generic,
				1 => Self::Function,
				4 => Self::UUID,
				5 => Self::MD5,
				6 => Self::Encrypted,
				_ => Self::UserDefined
			}
		}
	}
	
	#[derive(Serialize, Deserialize)]
	struct BinaryDataSerde<'a> {
		#[serde(rename = "$binary")]
		#[serde(borrow)]
		binary: BinaryDataSerdeInner<'a>
	}
	
	#[derive(Serialize, Deserialize)]
	struct BinaryDataSerdeInner<'a> {
		#[serde(with = "serde_bytes")]
		base64:  &'a [u8],
		subtype: BinarySubtype
	}
	
	impl<'a> Serialize for BinaryData<'a> {
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where
			S: Serializer {
			BinaryDataSerde {
				binary: BinaryDataSerdeInner {
					base64:  self.binary,
					subtype: self.r#type
				}
			}.serialize(serializer)
		}
	}
	
	impl<'de> Deserialize<'de> for BinaryData<'de> {
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where
			D: Deserializer<'de> {
			BinaryDataSerde::deserialize(deserializer).map(|v| Self {
				binary: v.binary.base64,
				r#type: v.binary.subtype
			})
		}
	}
}

mod utc_date_time {
	use super::*;
	use std::time::{SystemTime, UNIX_EPOCH};

	/// Utc date time represented as a signed 64-bit integer.
	#[repr(transparent)]
	#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
	pub struct UtcDateTime(pub i64);
	
	impl UtcDateTime {
		pub fn now() -> Self {
			Self(SystemTime::now().duration_since(UNIX_EPOCH)
				.unwrap().as_millis() as _)
		}
	}
	
	impl BsonValue<'_> for UtcDateTime {
		const ID: u8 = 0x09;
		
		fn write_bson(&self, buf: &mut [u8]) {
			buf.copy_from_slice(&self.0.to_le_bytes());
		}
		
		fn read_bson(buf: &[u8]) -> Self {
			Self(i64::from_le_bytes([
				buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]
			]))
		}
	}
	
	#[derive(Serialize, Deserialize)]
	struct UtcDateTimeSerde {
		#[serde(rename = "$date")]
		date: i64
	}
	
	impl Serialize for UtcDateTime {
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where
			S: Serializer {
			UtcDateTimeSerde { date: self.0 }.serialize(serializer)
		}
	}
	
	impl<'de> Deserialize<'de> for UtcDateTime {
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where
			D: Deserializer<'de> {
			UtcDateTimeSerde::deserialize(deserializer).map(|v| Self(v.date))
		}
	}
}

mod regex {
	use super::*;
	use crate::bson::de::read_str_nt;

	/// A Regular Expression consisting of a pattern and options.
	#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
	pub struct Regex<'a> {
		pub pattern: &'a str,
		pub options: &'a str
	}
	
	impl<'a> BsonValue<'a> for Regex<'a> {
		const ID: u8 = 0x0B;
		
		fn size(&self) -> usize {
			2 + self.pattern.len() + self.options.len()
		}
		
		fn write_bson(&self, mut buf: &mut [u8]) {
			buf[..self.pattern.len()].copy_from_slice(self.pattern.as_bytes());
			buf = &mut buf[self.pattern.len()..];
			buf[0] = 0;
			buf[1..=self.options.len()].copy_from_slice(self.options.as_bytes());
			buf = &mut buf[1 + self.options.len()..];
			buf[0] = 0;
		}
		
		fn read_bson(buf: &'a [u8]) -> Self {
			let pattern = read_str_nt(buf).unwrap();
			let options = read_str_nt(&buf[pattern.len() + 1..]).unwrap();
			Self { pattern, options }
		}
	}
	
	#[derive(Serialize, Deserialize)]
	struct RegexSerde<'a> {
		#[serde(rename = "$regularExpression")]
		#[serde(borrow)]
		regular_expression: RegexSerdeInner<'a>
	}
	
	#[derive(Serialize, Deserialize)]
	struct RegexSerdeInner<'a> {
		pattern: &'a str,
		options: &'a str
	}
	
	impl<'a> Serialize for Regex<'a> {
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where
			S: Serializer {
			RegexSerde {
				regular_expression: RegexSerdeInner {
					pattern: self.pattern,
					options: self.options
				}
			}.serialize(serializer)
		}
	}
	
	impl<'de> Deserialize<'de> for Regex<'de> {
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where
			D: Deserializer<'de> {
			RegexSerde::deserialize(deserializer).map(|v| Self {
				pattern: v.regular_expression.pattern,
				options: v.regular_expression.options
			})
		}
	}
	
}

mod script {
	use super::*;

	/// JavaScript code to be executed on the database server
	#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
	pub struct JavaScript<'a> {
		#[serde(rename = "$code")]
		pub code: &'a str
	}
	
	impl<'a> BsonValue<'a> for JavaScript<'a> {
		const ID: u8 = 0x0D;
		
		fn size(&self) -> usize {
			unimplemented!()
		}
		
		fn write_bson(&self, _buf: &mut [u8]) {
			unimplemented!()
		}
		
		fn read_bson(_buf: &'a [u8]) -> Self {
			unimplemented!()
		}
	}
	
	#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
	pub struct JavaScriptWithScope<'a> {
		#[serde(rename = "$code")]
		pub code:  &'a str,
		#[serde(rename = "$scope")]
		#[serde(with = "serde_bytes")]
		pub scope: &'a [u8]
	}
	
	impl<'a> BsonValue<'a> for JavaScriptWithScope<'a> {
		const ID: u8 = 0x0F;
		
		fn size(&self) -> usize {
			unimplemented!()
		}
		
		fn write_bson(&self, _buf: &mut [u8]) {
			unimplemented!()
		}
		
		fn read_bson(_buf: &'a [u8]) -> Self {
			unimplemented!()
		}
	}
}

mod timestamp {
	use super::*;

	/// A 64-bit unsigned timestamp
	#[repr(transparent)]
	#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
	pub struct Timestamp(pub u64);
	
	impl BsonValue<'_> for Timestamp {
		const ID: u8 = 0x11;

		fn write_bson(&self, buf: &mut [u8]) {
			buf[..8].copy_from_slice(&self.0.to_le_bytes())
		}

		fn read_bson(buf: &[u8]) -> Self {
			Self(u64::from_le_bytes([
				buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7]
			]))
		}
	}
	
	#[derive(Serialize, Deserialize)]
	struct TimestampSerde {
		#[serde(rename = "$timestamp")]
		timestamp: u64
	}
	
	impl Serialize for Timestamp {
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where
			S: Serializer {
			TimestampSerde { timestamp: self.0 }.serialize(serializer)
		}
	}
	
	impl<'de> Deserialize<'de> for Timestamp {
		fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where
			D: Deserializer<'de> {
			TimestampSerde::deserialize(deserializer).map(|v| Self(v.timestamp))
		}
	}
}
