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
	std::marker::PhantomData,
	serde::{ser::*, Serializer as _Ser}
};

pub fn serialize(v: impl Serialize, mut writer: impl io::Write) -> Result<(), Error> {
	let mut s = Serializer(Vec::new());
	v.serialize(&mut s)?;
	writer.write_all(s.0.as_slice()).map_err(Error::Io)
}

#[derive(Debug)]
pub enum Error {
	Io(std::io::Error),
	Unknown(String),
	InvalidType,
	InvalidIdentifier(&'static str)
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		std::fmt::Debug::fmt(self, f)
	}
}

impl std::error::Error for Error {}

impl serde::ser::Error for Error {
	fn custom<T: std::fmt::Display>(msg: T) -> Self {
		Self::Unknown(msg.to_string())
	}
}

pub struct Serializer(pub(super) Vec<u8>);

impl<'a> serde::Serializer for &'a mut Serializer {
	type Ok                     = u8;
	type Error                  = Error;
	type SerializeSeq           = DocSerializer<'a>;
	type SerializeTuple         = DocSerializer<'a>;
	type SerializeTupleStruct   = DocSerializer<'a>;
	type SerializeTupleVariant  = DocSerializer<'a>;
	type SerializeMap           = DocSerializer<'a>;
	type SerializeStruct        = DocSerializer<'a>;
	type SerializeStructVariant = DocSerializer<'a>;
	
	fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
		self.0.push(if v { 1 } else { 0 });
		Ok(bool::ID)
	}
	
	fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
		self.0.extend_from_slice(&v.to_le_bytes());
		Ok(i32::ID)
	}
	
	fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
		self.0.extend_from_slice(&v.to_le_bytes());
		Ok(i64::ID)
	}
	
	fn serialize_i8(self, v: i8)   -> Result<Self::Ok, Self::Error> { self.serialize_i32(v as _) }
	fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> { self.serialize_i32(v as _) }
	fn serialize_u8(self, v: u8)   -> Result<Self::Ok, Self::Error> { self.serialize_i32(v as _) }
	fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> { self.serialize_i32(v as _) }
	fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> { self.serialize_i32(v as _) }
	fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> { self.serialize_i64(v as _) }
	fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> { self.serialize_f64(v as _) }
	
	fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
		self.0.extend_from_slice(&v.to_le_bytes());
		Ok(f64::ID)
	}
	
	fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
		self.serialize_str(&v.to_string())
	}
	
	fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
		self.serialize_i32((v.len() + 1) as _)?;
		self.0.extend_from_slice(v.as_bytes());
		self.0.push(0);
		Ok(<&str>::ID)
	}
	
	fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
		self.serialize_i32(v.len() as _)?;
		self.0.push(0);
		self.0.extend_from_slice(v);
		Ok(<&[u8]>::ID)
	}
	
	fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
		self.serialize_unit()
	}
	
	fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error> where
		T: serde::Serialize {
		value.serialize(self)
	}
	
	fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
		Ok(<()>::ID)
	}
	
	fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
		self.serialize_unit()
	}
	
	fn serialize_unit_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str) -> Result<Self::Ok, Self::Error> {
		self.serialize_unit()
	}
	
	fn serialize_newtype_struct<T: ?Sized>(self, _name: &'static str, value: &T) -> Result<Self::Ok, Self::Error> where
		T: serde::Serialize {
		value.serialize(self)
	}
	
	fn serialize_newtype_variant<T: ?Sized>(self, _name: &'static str, _variant_index: u32, _variant: &'static str, value: &T) -> Result<Self::Ok, Self::Error> where
		T: serde::Serialize {
		value.serialize(self)
	}
	
	fn serialize_seq(self, _len: Option<usize>)                                                                      -> Result<Self::SerializeSeq, Self::Error>           { DocSerializer::new(self, 0x4) }
	fn serialize_tuple(self, _len: usize)                                                                            -> Result<Self::SerializeTuple, Self::Error>         { DocSerializer::new(self, 0x4) }
	fn serialize_tuple_struct(self, _name: &'static str, _len: usize)                                                -> Result<Self::SerializeTupleStruct, Self::Error>   { DocSerializer::new(self, 0x4) }
	fn serialize_tuple_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize)  -> Result<Self::SerializeTupleVariant, Self::Error>  { DocSerializer::new(self, 0x4) }
	fn serialize_map(self, _len: Option<usize>)                                                                      -> Result<Self::SerializeMap, Self::Error>           { DocSerializer::new(self, 0x3) }
	fn serialize_struct(self, _name: &'static str, _len: usize)                                                      -> Result<Self::SerializeStruct, Self::Error>        { DocSerializer::new(self, 0x3) }
	fn serialize_struct_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> Result<Self::SerializeStructVariant, Self::Error> { DocSerializer::new(self, 0x3) }
	
	fn is_human_readable(&self) -> bool {
		false
	}
}

pub struct DocSerializer<'a> {
	inner:  &'a mut Serializer,
	offset: usize,
	index:  usize,
	r#type: u8
}

impl<'a> DocSerializer<'a> {
	fn new(ser: &'a mut Serializer, r#type: u8) -> Result<Self, Error> {
		Ok(Self {
			offset: ser.0.len(),
			inner:  {
				ser.serialize_i32(0)?; // length
				ser
			},
			index:  0,
			r#type
		})
	}
}

impl<'a> SerializeMap for DocSerializer<'a> {
	type Ok    = u8;
	type Error = Error;
	
	fn serialize_key<T: ?Sized>(&mut self, _key: &T) -> Result<(), Self::Error> where
		T: Serialize {
		unimplemented!()
	}
	
	fn serialize_value<T: ?Sized>(&mut self, _value: &T) -> Result<(), Self::Error> where
		T: Serialize {
		unimplemented!()
	}
	
	fn serialize_entry<K: ?Sized, V: ?Sized>(&mut self, key: &K, value: &V) -> Result<(), Self::Error> where
		K: Serialize,
		V: Serialize, {
		let key = key.serialize(AsString)?;
		if key.starts_with('$') {
			#[allow(clippy::never_loop)]
			'l: loop {
				self.r#type = match key.as_str() {
					"$binary" => value.serialize(
						&mut SpecialSerializer::<BinaryData>::new(self.inner))
							.map(|_| BinaryData::ID),
					"$oid" => value.serialize(
						&mut SpecialSerializer::<ObjectId>::new(self.inner))
							.map(|_| ObjectId::ID),
					"$date" => value.serialize(
						&mut SpecialSerializer::<UtcDateTime>::new(self.inner))
							.map(|_| UtcDateTime::ID),
					"$regularExpression" => value.serialize(
						&mut SpecialSerializer::<Regex>::new(self.inner))
							.map(|_| Regex::ID),
					"$code" => {
						let code = value.serialize(AsString)?;
						self.inner.serialize_str(code.as_str())?;
						self.inner.0.extend_from_slice(code.as_bytes());
						self.inner.0.push(0);
						Ok(JavaScript::ID)
					}
					"$scope" => value.serialize(&mut *self.inner)
						.map(|_| JavaScriptWithScope::ID),
					"$timestamp" => value.serialize(
						&mut SpecialSerializer::<Timestamp>::new(self.inner))
						.map(|_| Timestamp::ID),
					// we don't want a document to be serialized as a byte array
					"$document" =>  value.serialize(
						&mut SpecialSerializer::<Document>::new(self.inner))
						.map(|_| Document::ID),
					_ => break 'l
				}?;
				return Ok(());
			}
		}
		
		if self.r#type != 0x03 && self.r#type != 0x04 {
			return Err(Error::InvalidIdentifier("invalid identifier for non-document type"));
		}
		
		let off = self.inner.0.len();
		self.inner.0.push(0);
		self.inner.0.extend_from_slice(key.as_bytes());
		self.inner.0.push(0);
		let id = value.serialize(&mut *self.inner)?;
		self.inner.0[off] = id;
		Ok(())
	}
	
	fn end(self) -> Result<Self::Ok, Self::Error> {
		match self.r#type {
			0x3 | 0x4 => {
				self.inner.0.push(0);
				let len = (self.inner.0.len() - self.offset) as i32;
				self.inner.0[self.offset..self.offset + 4].copy_from_slice(&len.to_le_bytes());
			},
			_ => ()
		}
		
		Ok(self.r#type)
	}
}

impl<'a> SerializeStruct for DocSerializer<'a> {
	type Ok    = u8;
	type Error = Error;
	
	fn serialize_field<T: ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error> where
		T: Serialize {
		self.serialize_entry(key, value)
	}
	
	fn end(self) -> Result<Self::Ok, Self::Error> {
		SerializeMap::end(self)
	}
}

impl<'a> SerializeStructVariant for DocSerializer<'a> {
	type Ok    = u8;
	type Error = Error;
	
	fn serialize_field<T: ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error> where
		T: Serialize {
		self.serialize_entry(key, value)
	}
	
	fn end(self) -> Result<Self::Ok, Self::Error> {
		SerializeMap::end(self)
	}
}

impl<'a> SerializeSeq for DocSerializer<'a> {
	type Ok    = u8;
	type Error = Error;
	
	fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> where
		T: Serialize {
		let __tmp__ = self.index;
		self.serialize_entry(&__tmp__, value)?;
		self.index += 1;
		Ok(())
	}
	
	fn end(self) -> Result<Self::Ok, Self::Error> {
		SerializeMap::end(self)
	}
}

impl<'a> SerializeTuple for DocSerializer<'a> {
	type Ok    = u8;
	type Error = Error;
	
	fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> where
		T: Serialize {
		SerializeSeq::serialize_element(self, value)
	}
	
	fn end(self) -> Result<Self::Ok, Self::Error> {
		SerializeMap::end(self)
	}
}

impl<'a> SerializeTupleStruct for DocSerializer<'a> {
	type Ok    = u8;
	type Error = Error;
	
	fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> where
		T: Serialize {
		SerializeSeq::serialize_element(self, value)
	}
	
	fn end(self) -> Result<Self::Ok, Self::Error> {
		SerializeMap::end(self)
	}
}

impl<'a> SerializeTupleVariant for DocSerializer<'a> {
	type Ok    = u8;
	type Error = Error;
	
	fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> where
		T: Serialize {
		SerializeSeq::serialize_element(self, value)
	}
	
	fn end(self) -> Result<Self::Ok, Self::Error> {
		SerializeMap::end(self)
	}
}

macro_rules! forward_serialize {
	( $expr:expr; $( $ident:ident ),* ) => { $( forward_serialize_helper!($expr; $ident); )* };
}

macro_rules! forward_serialize_helper {
    ($expr:expr; bool) => { forward_serialize_method!{$expr; serialize_bool(v: bool)} };
    ($expr:expr; i8) => { forward_serialize_method!{$expr; serialize_i8(v: i8)} };
    ($expr:expr; i16) => { forward_serialize_method!{$expr; serialize_i16(v: i16)} };
    ($expr:expr; i32) => { forward_serialize_method!{$expr; serialize_i32(v: i32)} };
    ($expr:expr; i64) => { forward_serialize_method!{$expr; serialize_i64(v: i64)} };
    ($expr:expr; u8) => { forward_serialize_method!{$expr; serialize_u8(v: u8)} };
    ($expr:expr; u16) => { forward_serialize_method!{$expr; serialize_u16(v: u16)} };
    ($expr:expr; u32) => { forward_serialize_method!{$expr; serialize_u32(v: u32)} };
    ($expr:expr; u64) => { forward_serialize_method!{$expr; serialize_u64(v: u64)} };
    ($expr:expr; f32) => { forward_serialize_method!{$expr; serialize_f32(v: f32)} };
    ($expr:expr; f64) => { forward_serialize_method!{$expr; serialize_f64(v: f64)} };
    ($expr:expr; char) => { forward_serialize_method!{$expr; serialize_char(v: char)} };
    ($expr:expr; str) => { forward_serialize_method!{$expr; serialize_str(v: &str)} };
    ($expr:expr; bytes) => { forward_serialize_method!{$expr; serialize_bytes(v: &[u8])} };
    ($expr:expr; none) => { forward_serialize_method!{$expr; serialize_none()} };
    ($expr:expr; unit) => { forward_serialize_method!{$expr; serialize_unit()} };
    ($expr:expr; unit_struct) => { forward_serialize_method!{$expr; serialize_unit_struct(name: &'static str)} };
    ($expr:expr; unit_variant) => { forward_serialize_method!{$expr; serialize_unit_variant(name: &'static str, variant_index: u32, variant: &'static str)} };
    ($expr:expr; seq) => { forward_serialize_method!{$expr; serialize_seq(len: Option<usize>) -> Self::SerializeSeq} };
    ($expr:expr; tuple) => { forward_serialize_method!{$expr; serialize_tuple(len: usize) -> Self::SerializeTuple} };
    ($expr:expr; tuple_struct) => { forward_serialize_method!{$expr; serialize_tuple_struct(name: &'static str, len: usize) -> Self::SerializeTupleStruct} };
    ($expr:expr; tuple_variant) => { forward_serialize_method!{$expr; serialize_tuple_variant(name: &'static str, variant_index: u32, variant: &'static str, len: usize) -> Self::SerializeTupleVariant} };
    ($expr:expr; map) => { forward_serialize_method!{$expr; serialize_map(len: Option<usize>) -> Self::SerializeMap} };
    ($expr:expr; struct) => { forward_serialize_method!{$expr; serialize_struct(name: &'static str, len: usize) -> Self::SerializeStruct} };
    ($expr:expr; struct_variant) => { forward_serialize_method!{$expr; serialize_struct_variant(name: &'static str, variant_index: u32, variant: &'static str, len: usize) -> Self::SerializeStructVariant} };
    ($expr:expr; some) => {
		fn serialize_some<U: ?Sized>(self, _value: &U) -> Result<Self::Ok, Self::Error> where
				U: Serialize { $expr }
    };
    ($expr:expr; newtype_struct) => {
		fn serialize_newtype_struct<U: ?Sized>(self, _name: &'static str, _value: &U) -> Result<Self::Ok, Self::Error> where
			U: Serialize { $expr }
    };
    ($expr:expr; newtype_variant) => {
		fn serialize_newtype_variant<U: ?Sized>(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _value: &U) -> Result<Self::Ok, Self::Error> where
			U: Serialize { $expr }
    };
}

macro_rules! forward_serialize_method {
	($expr:expr; $func:ident( $( $arg:ident : $ty:ty ),* ) -> $r:ty) => {
		fn $func(self, $( $arg: $ty, )* ) -> Result<$r, Self::Error> {
			$( let _ = $arg; )*
			$expr
        }
    };
    ($expr:expr; $func:ident( $( $arg:ident : $ty:ty ),* )) => {
		forward_serialize_method!($expr; $func( $( $arg : $ty ),* ) -> Self::Ok);
    };
}

struct SpecialSerializer<'a, T> {
	inner:  &'a mut Serializer,
	marker: PhantomData<T>
}

impl<'a, T> SpecialSerializer<'a, T> {
	fn new(ser: &'a mut Serializer) -> Self {
		ser.0.resize(ser.0.len() - 4, 0); // remove doc length as this type isn't a document
		Self { inner: ser, marker: PhantomData }
	}
}

impl<'a, 'b> SerializeStruct for &'b mut SpecialSerializer<'a, BinaryData<'a>> {
	type Ok    = ();
	type Error = Error;
	
	fn serialize_field<T: ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error> where
		T: Serialize {
		match key {
			"subtype" | "base64" => value.serialize(&mut**self),
			_ => Err(Error::InvalidIdentifier(key))
		}
	}
	
	fn end(self) -> Result<Self::Ok, Self::Error> {
		Ok(())
	}
}

impl<'a, 'b> serde::Serializer for &'b mut SpecialSerializer<'a, BinaryData<'a>> {
	type Ok                     = ();
	type Error                  = Error;
	type SerializeSeq           = Impossible<(), Error>;
	type SerializeTuple         = Impossible<(), Error>;
	type SerializeTupleStruct   = Impossible<(), Error>;
	type SerializeTupleVariant  = Impossible<(), Error>;
	type SerializeMap           = Impossible<(), Error>;
	type SerializeStruct        = Self;
	type SerializeStructVariant = Impossible<(), Error>;
	
	fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
		let __tmp__ = self.inner.0.len();
		self.inner.0[__tmp__ - 5..__tmp__ - 1]
			.copy_from_slice(&(v.len() as i32).to_le_bytes());
		self.inner.0.extend_from_slice(v);
		Ok(())
	}
	
	fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct, Self::Error> {
		Ok(self)
	}
	
	fn serialize_unit_variant(self, _name: &'static str, variant_index: u32, _variant: &'static str) -> Result<Self::Ok, Self::Error> {
		self.inner.0.extend_from_slice(&[0, 0, 0, 0, variant_index as _]);
		Ok(())
	}
	
	forward_serialize!(
		Err(Error::InvalidType); bool, i8, i16, i32, i64, u8, u16, u32, u64, f32, f64, char, str,
		none, unit, unit_struct, seq, tuple, tuple_struct, tuple_variant, map,
		struct_variant, some, newtype_struct, newtype_variant
	);
}

impl<'a, 'b> serde::Serializer for &'b mut SpecialSerializer<'a, ObjectId> {
	type Ok                     = ();
	type Error                  = Error;
	type SerializeSeq           = Impossible<(), Error>;
	type SerializeTuple         = Impossible<(), Error>;
	type SerializeTupleStruct   = Impossible<(), Error>;
	type SerializeTupleVariant  = Impossible<(), Error>;
	type SerializeMap           = Impossible<(), Error>;
	type SerializeStruct        = Impossible<(), Error>;
	type SerializeStructVariant = Impossible<(), Error>;
	
	fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
		debug_assert_eq!(v.len(), 12);
		self.inner.0.extend_from_slice(v);
		Ok(())
	}
	
	forward_serialize!(
		Err(Error::InvalidType); bool, i8, i16, i32, i64, u8, u16, u32, u64, f32, f64, char, str,
		none, unit, unit_struct, unit_variant, seq, tuple, tuple_struct, tuple_variant, map,
		struct, struct_variant, some, newtype_struct, newtype_variant
	);
}

impl<'a, 'b> serde::Serializer for &'b mut SpecialSerializer<'a, UtcDateTime> {
	type Ok                     = ();
	type Error                  = Error;
	type SerializeSeq           = Impossible<(), Error>;
	type SerializeTuple         = Impossible<(), Error>;
	type SerializeTupleStruct   = Impossible<(), Error>;
	type SerializeTupleVariant  = Impossible<(), Error>;
	type SerializeMap           = Impossible<(), Error>;
	type SerializeStruct        = Impossible<(), Error>;
	type SerializeStructVariant = Impossible<(), Error>;
	
	fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
		self.inner.0.extend_from_slice(&v.to_le_bytes());
		Ok(())
	}
	
	forward_serialize!(
		Err(Error::InvalidType); bool, i8, i16, i32, u8, u16, u32, u64, f32, f64, char, str, bytes,
		none, unit, unit_struct, unit_variant, seq, tuple, tuple_struct, tuple_variant, map,
		struct, struct_variant, some, newtype_struct, newtype_variant
	);
}

impl<'a, 'b> SerializeStruct for &'b mut SpecialSerializer<'a, Regex<'a>> {
	type Ok    = ();
	type Error = Error;
	
	fn serialize_field<T: ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), Self::Error> where
		T: Serialize {
		match key {
			"pattern" | "options" => value.serialize(&mut**self),
			_ => Err(Error::InvalidIdentifier(key))
		}
	}
	
	fn end(self) -> Result<Self::Ok, Self::Error> {
		Ok(())
	}
}

impl<'a, 'b> serde::Serializer for &'b mut SpecialSerializer<'a, Regex<'a>> {
	type Ok                     = ();
	type Error                  = Error;
	type SerializeSeq           = Impossible<(), Error>;
	type SerializeTuple         = Impossible<(), Error>;
	type SerializeTupleStruct   = Impossible<(), Error>;
	type SerializeTupleVariant  = Impossible<(), Error>;
	type SerializeMap           = Impossible<(), Error>;
	type SerializeStruct        = Self;
	type SerializeStructVariant = Impossible<(), Error>;
	
	fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
		self.inner.0.extend_from_slice(v.as_bytes());
		self.inner.0.push(0);
		Ok(())
	}
	
	fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct, Self::Error> {
		Ok(self)
	}
	
	forward_serialize!(
		Err(Error::InvalidType); bool, i8, i16, i32, i64, u8, u16, u32, u64, f32, f64, char, bytes,
		none, unit, unit_struct, unit_variant, seq, tuple, tuple_struct, tuple_variant, map,
		struct_variant, some, newtype_struct, newtype_variant
	);
}

impl<'a, 'b> serde::Serializer for &'b mut SpecialSerializer<'a, Timestamp> {
	type Ok                     = ();
	type Error                  = Error;
	type SerializeSeq           = Impossible<(), Error>;
	type SerializeTuple         = Impossible<(), Error>;
	type SerializeTupleStruct   = Impossible<(), Error>;
	type SerializeTupleVariant  = Impossible<(), Error>;
	type SerializeMap           = Impossible<(), Error>;
	type SerializeStruct        = Impossible<(), Error>;
	type SerializeStructVariant = Impossible<(), Error>;
	
	fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
		self.inner.0.extend_from_slice(&v.to_le_bytes());
		Ok(())
	}
	
	forward_serialize!(
		Err(Error::InvalidType); bool, i8, i16, i32, i64, u8, u16, u32, f32, f64, char, str, bytes,
		none, unit, unit_struct, unit_variant, seq, tuple, tuple_struct, tuple_variant, map,
		struct, struct_variant, some, newtype_struct, newtype_variant
	);
}

impl<'a, 'b> serde::Serializer for &'b mut SpecialSerializer<'a, Document> {
	type Ok                     = ();
	type Error                  = Error;
	type SerializeSeq           = Impossible<(), Error>;
	type SerializeTuple         = Impossible<(), Error>;
	type SerializeTupleStruct   = Impossible<(), Error>;
	type SerializeTupleVariant  = Impossible<(), Error>;
	type SerializeMap           = Impossible<(), Error>;
	type SerializeStruct        = Impossible<(), Error>;
	type SerializeStructVariant = Impossible<(), Error>;
	
	fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
		self.inner.serialize_i32(0)?;
		self.inner.0.extend_from_slice(&v[..v.len() - 1]);
		Ok(())
	}
	
	forward_serialize!(
		Err(Error::InvalidType); bool, i8, i16, i32, i64, u8, u16, u32, u64, f32, f64, char, str,
		none, unit, unit_struct, unit_variant, seq, tuple, tuple_struct, tuple_variant, map,
		struct, struct_variant, some, newtype_struct, newtype_variant
	);
}

struct AsString;

impl<'a> serde::Serializer for AsString {
	type Ok                     = String;
	type Error                  = Error;
	type SerializeSeq           = Impossible<String, Error>;
	type SerializeTuple         = Impossible<String, Error>;
	type SerializeTupleStruct   = Impossible<String, Error>;
	type SerializeTupleVariant  = Impossible<String, Error>;
	type SerializeMap           = Impossible<String, Error>;
	type SerializeStruct        = Impossible<String, Error>;
	type SerializeStructVariant = Impossible<String, Error>;
	
	fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> { Ok(if v { "true" } else { "false" }.to_string()) }
	fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> { Ok(v.to_string()) }
	fn serialize_none(self) -> Result<Self::Ok, Self::Error> { self.serialize_unit() }
	
	fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error> where
		T: Serialize {
		value.serialize(self)
	}
	
	fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
		Ok("null".to_string())
	}
	
	fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
		self.serialize_unit()
	}
	
	forward_serialize!(
		Err(Error::InvalidType); bytes, unit_variant, seq, tuple, tuple_struct, tuple_variant, map,
		struct, struct_variant, newtype_struct, newtype_variant
	);
}