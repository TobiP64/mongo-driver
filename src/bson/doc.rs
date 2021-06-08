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

use super::{*, de::read_str_nt};
use serde::{Deserialize, ser::{Serialize, SerializeMap, SerializeSeq}};

pub use self::{builder::*, doc::*, array::*, doc_ref::*, array_ref::*, doc_mut::*, entry::*};

mod builder {
	use super::*;

	pub struct DocBuilder(Vec<u8>);

	impl DocBuilder {
		pub fn new() -> Self {
			Self(Vec::new())
		}

		pub fn append<'a, T: BsonValue<'a>>(mut self, key: &str, value: T) -> Self {
			self.0.push(T::ID);
			self.0.extend_from_slice(key.as_bytes());
			self.0.push(0);
			self.0.extend_from_slice(vec![0u8; value.size()].as_slice());
			let __tmp__ = self.0.len();
			value.write_bson(&mut self.0[__tmp__ - value.size()..]);
			self
		}

		pub fn append_any(self, key: &str, value: Bson) -> Self {
			match value {
				Bson::Unit => self.append(key, ()),
				Bson::Bool(v) => self.append(key, v),
				Bson::I32(v) => self.append(key, v),
				Bson::I64(v) => self.append(key, v),
				Bson::F64(v) => self.append(key, v),
				Bson::F128(v) => self.append(key, v),
				Bson::ObjectId(v) => self.append(key, v),
				Bson::Timestamp(v) => self.append(key, v),
				Bson::Document(v) => self.append(key, v),
				Bson::Array(v) => self.append(key, v),
				Bson::String(v) => self.append(key, v),
				Bson::Binary(v) => self.append(key, v),
				Bson::Datetime(v) => self.append(key, v),
				Bson::Regex(v) => self.append(key, v),
				Bson::JavaScript(v) => self.append(key, v),
				Bson::JavaScriptWithScope(v) => self.append(key, v),
			}
		}

		pub fn finish(mut self) -> Document {
			self.0.push(0);
			for _ in 0..4 { self.0.insert(0, 0); }
			let __tmp__ = self.0.len();
			self.0[..4].copy_from_slice(&(__tmp__ as u32).to_le_bytes());
			Document(self.0)
		}
	}

	impl Default for DocBuilder {
		fn default() -> Self {
			Self::new()
		}
	}
}

#[allow(clippy::module_inception)]
mod doc {
	use {
		super::*,
		serde::de::DeserializeOwned,
		std::iter::FromIterator
	};

	/// A BSON document
	///
	/// # Structure
	/// ```no-run
	/// [0..4] length encoded as a little endian i32
	/// [4..x] data
	/// [x..x+1] 0 (terminator)
	/// ```
	#[allow(rustdoc::invalid_codeblock_attributes)]
	#[derive(Clone, Eq, PartialEq, Deserialize)]
	pub struct Document(#[serde(with = "serde_bytes")] pub Vec<u8>);
	
	impl Document {
		pub fn new() -> Self {
			Self(vec![5u8, 0u8, 0u8, 0u8, 0u8])
		}
		
		pub fn from(v: &impl Serialize) -> Result<Document, se::Error> {
			let mut d = se::Serializer(Vec::new());
			v.serialize(&mut d)?;
			Ok(Self(d.0))
		}

		pub fn into<T: DeserializeOwned>(self) -> Result<T, de::Error> {
			T::deserialize(&mut de::Deserializer::new(self.0.as_slice()))
		}
		
		pub fn copy_from_slice(src: &[u8]) -> Self {
			Self(src[..u32::from_le_bytes([src[0], src[1], src[2], src[3]]) as usize].to_vec())
		}

		pub fn deserialize<'de, T: Deserialize<'de>>(&'de self) -> Result<T, de::Error> {
			T::deserialize(&mut de::Deserializer::new(&self.0))
		}

		pub fn as_ref(&self) -> DocRef {
			self.into()
		}

		pub fn as_mut(&mut self) -> DocMut {
			DocMut { buf: &mut self.0, out: None, off: 0 }
		}

		// immutable functions

		pub fn iter(&self) -> DocRefIter {
			self.into_iter()
		}

		pub fn len(&self) -> usize {
			self.into_iter().count()
		}

		pub fn is_empty(&self) -> bool {
			self.0.len() == 5
		}

		pub fn contains<'b, T: BsonValue<'b>>(&self, key: &str) -> bool {
			self.as_ref().contains::<T>(key)
		}

		pub fn contains_any(&self, key: &str) -> bool {
			self.as_ref().contains_any(key)
		}

		pub fn get<'b, T: std::convert::TryFrom<Bson<'b>> + 'b>(&'b self, key: &str) -> Option<T> {
			self.as_ref().get(key)
		}

		pub fn get_any<'b>(&'b self, key: &str) -> Option<Bson<'b>> {
			self.as_ref().get_any(key)
		}

		// mutable functions

		pub fn append<'b, T: BsonValue<'b>>(&mut self, key: &str, value: &T) {
			self.as_mut().append(key, value)
		}

		pub fn insert<'b, T: BsonValue<'b>>(&mut self, key: &str, value: &T) -> bool {
			self.as_mut().insert(key, value)
		}

		pub fn remove(&mut self, key: &str) -> bool {
			self.as_mut().remove(key)
		}

		pub fn clear(&mut self) {
			self.as_mut().clear()
		}
	}
	
	impl Default for Document {
		fn default() -> Self {
			Self::new()
		}
	}
	
	impl std::ops::Deref for Document {
		type Target = Vec<u8>;

		fn deref(&self) -> &Self::Target {
			&self.0
		}
	}
	
	impl std::ops::DerefMut for Document {
		fn deref_mut(&mut self) -> &mut Self::Target {
			&mut self.0
		}
	}
	
	impl Serialize for Document {
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where
			S: Serializer {
			self.as_ref().serialize(serializer)
		}
	}
	
	impl<'a, T: BsonValue<'a>> FromIterator<(&'a str, T)> for Document {
		fn from_iter<I: IntoIterator<Item = (&'a str, T)>>(iter: I) -> Self {
			let mut builder = DocBuilder::new();
			for (k, v) in iter.into_iter() { builder = builder.append(k, v); }
			builder.finish()
		}
	}
	
	impl<'a> FromIterator<(&'a str, Bson<'a>)> for Document {
		fn from_iter<T: IntoIterator<Item = (&'a str, Bson<'a>)>>(iter: T) -> Self {
			let mut builder = DocBuilder::new();
			for (k, v) in iter.into_iter() { builder = builder.append_any(k, v); }
			builder.finish()
		}
	}
	
	impl<'a, T: BsonValue<'a>> FromIterator<T> for Document {
		fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
			let mut builder = DocBuilder::new();
			for (k, v) in iter.into_iter().enumerate() { builder = builder.append(&k.to_string(), v); }
			builder.finish()
		}
	}
	
	impl<'a> FromIterator<Bson<'a>> for Document {
		fn from_iter<T: IntoIterator<Item = Bson<'a>>>(iter: T) -> Self {
			let mut builder = DocBuilder::new();
			for (k, v) in iter.into_iter().enumerate() { builder = builder.append_any(&k.to_string(), v); }
			builder.finish()
		}
	}
	
	impl<'a> IntoIterator for &'a Document {
		type Item     = (&'a str, Bson<'a>);
		type IntoIter = DocRefIter<'a>;

		fn into_iter(self) -> Self::IntoIter {
			DocRefIter(&self.0[4..])
		}
	}
	
	impl std::fmt::Debug for Document {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			let mut f = f.debug_map();
			self.into_iter().for_each(|(k, v)| { f.entry(&k, &v); });
			f.finish()
		}
	}
	
	impl std::fmt::Display for Document {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			std::fmt::Debug::fmt(self, f)
		}
	}
}

mod array {
	use super::*;
	
	#[derive(Clone, Eq, PartialEq, Deserialize)]
	pub struct Array(#[serde(with = "serde_bytes")] pub Vec<u8>);
}

mod doc_ref {
	use {
		super::*,
		std::convert::TryFrom
	};

	#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
	pub struct DocRef<'a>(pub &'a [u8]);
	
	impl<'a> DocRef<'a> {
		pub fn iter(&self) -> DocRefIter {
			self.into_iter()
		}

		pub fn len(&self) -> usize {
			self.into_iter().count()
		}

		pub fn is_empty(&self) -> bool {
			self.0.len() > 1
		}

		pub fn contains<'b, T: BsonValue<'b>>(&self, key: &str) -> bool {
			self.into_iter().any(|(k, v)| k == key && v.id() == T::ID)
		}

		pub fn contains_any(&self, key: &str) -> bool {
			self.into_iter().any(|(k, _)| k == key)
		}
		
		pub fn get<T: TryFrom<Bson<'a>> + 'a>(&self, key: &str) -> Option<T> {
			self.get_any(key).and_then(|v| T::try_from(v).ok())
		}

		pub fn get_any(&self, key: &str) -> Option<Bson<'a>> {
			self.into_iter().find(|(k, _)| *k == key).map(|(_, v)| v)
		}

		pub fn deserialize<'de, T: Deserialize<'de>>(&'de self) -> Result<T, de::Error> {
			T::deserialize(&mut de::Deserializer::new(self.0))
		}
	}
	
	impl<'a> From<&'a Document> for DocRef<'a> {
		fn from(v: &'a Document) -> Self {
			Self(&v.0[4..])
		}
	}
	
	impl<'a> Into<Document> for DocRef<'a> {
		fn into(self) -> Document {
			let mut vec = (self.0.len() as u32 + 4).to_le_bytes().to_vec();
			vec.extend(self.0);
			Document(vec)
		}
	}
	
	impl<'a> IntoIterator for DocRef<'a> {
		type Item     = (&'a str, Bson<'a>);
		type IntoIter = DocRefIter<'a>;
		
		fn into_iter(self) -> Self::IntoIter {
			DocRefIter(self.0)
		}
	}
	
	pub struct DocRefIter<'a>(pub(super) &'a [u8]);
	
	impl<'a> Iterator for DocRefIter<'a> {
		type Item = (&'a str, Bson<'a>);
		
		fn next(&mut self) -> Option<Self::Item> {
			let id = self.0[0];
			if id == 0 { return None; }
			let key = read_str_nt(&self.0[1..]).unwrap();
			self.0 = &self.0[2 + key.len()..];
			let val = Bson::read(id, self.0).unwrap();
			self.0 = &self.0[get_value_size(id, self.0)..];
			Some((key, val))
		}
	}
	
	#[derive(Serialize, Deserialize)]
	struct DocSerde<'a> {
		#[serde(rename = "$document")]
		#[serde(with = "serde_bytes")]
		doc: &'a [u8]
	}
	
	impl<'a> Serialize for DocRef<'a> {
		fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error> where
			S: Serializer {
			if serializer.is_human_readable() {
				let mut serializer = serializer.serialize_map(None)?;
				for (k, v) in DocRefIter(self.0) {
					serializer.serialize_entry(k, &v)?;
				}
				serializer.end()
			} else {
				DocSerde { doc: self.0 }.serialize(serializer)
			}
		}
	}
	
	impl std::fmt::Debug for DocRef<'_> {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			let mut f = f.debug_map();
			DocRefIter(self.0).for_each(|(k, v)| { f.entry(&k, &v); });
			f.finish()
		}
	}
	
	impl std::fmt::Display for DocRef<'_> {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			std::fmt::Debug::fmt(self, f)
		}
	}
}

mod array_ref {
	use super::*;
	
	#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
	pub struct ArrayRef<'a>(pub &'a [u8]);
	
	impl<'a> From<&'a Array> for ArrayRef<'a> {
		fn from(v: &'a Array) -> Self {
			Self(&v.0[4..])
		}
	}

	impl<'a> Into<Document> for ArrayRef<'a> {
		fn into(self) -> Document {
			let mut vec = (self.0.len() as u32 + 4).to_le_bytes().to_vec();
			vec.extend(self.0);
			Document(vec)
		}
	}

	#[derive(Serialize, Deserialize)]
	struct ArraySerde<'a> {
		#[serde(rename = "$array")]
		#[serde(with = "serde_bytes")]
		array: &'a [u8]
	}
	
	impl<'a> Serialize for ArrayRef<'a> {
		fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error> where
			S: Serializer {
			if serializer.is_human_readable() {
				let mut serializer = serializer.serialize_seq(None)?;
				for (_, v) in DocRefIter(self.0) {
					serializer.serialize_element(&v)?;
				}
				serializer.end()
			} else {
				ArraySerde { array: self.0 }.serialize(serializer)
			}
		}
	}
	
	impl std::fmt::Debug for ArrayRef<'_> {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			let mut f = f.debug_list();
			DocRefIter(self.0).for_each(|(_, v)| { f.entry(&v); });
			f.finish()
		}
	}
	
	impl std::fmt::Display for ArrayRef<'_> {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			std::fmt::Debug::fmt(self, f)
		}
	}
}

mod doc_mut {
	use super::*;
	
	#[derive(Debug, PartialEq)]
	pub struct DocMut<'a> {
		pub(super) buf: &'a mut Vec<u8>,
		pub(super) out: Option<&'a mut Self>,
		pub(super) off: usize
	}
	
	impl<'a> DocMut<'a> {
		pub fn size(&self) -> usize {
			i32::from_le_bytes([
				self.buf[self.off], self.buf[self.off + 1],
				self.buf[self.off + 2], self.buf[self.off + 3]
			]) as _
		}
		
		pub(super) fn set_len(&mut self, len: usize) {
			let diff = self.size() as i32 - len as i32;
			self.buf[self.off..self.off + 4].copy_from_slice(&(len as u32).to_le_bytes());
			let mut doc = self;
			while let Some(outer) = &mut doc.out {
				let len = outer.size() as i32;
				outer.buf[outer.off..outer.off + 4].copy_from_slice(&(len - diff).to_le_bytes());
				doc = *outer;
			}
		}
		
		fn find<'b>(&'b mut self, key: &str) -> Option<OccupiedEntry<'b, 'a>> {
			let mut off = self.off + 4;
			
			loop {
				let id = self.buf[off];
				
				if id == 0 {
					break;
				}
				
				let key_off = off;
				while self.buf[off] != 0 { off += 1; }
				
				if std::str::from_utf8(&self.buf[key_off + 1..off]).unwrap() != key {
					off += 1 + get_value_size(id, &self.buf[off + 1..]);
					continue;
				}
				
				return Some(OccupiedEntry {
					doc:     self,
					key_off,
					val_off: off + 1
				});
			}
			
			None
		}

		pub fn len(&self) -> usize {
			self.iter().count()
		}

		pub fn is_empty(&self) -> bool {
			self.size() == 0
		}

		pub fn iter(&self) -> DocRefIter {
			DocRefIter(&self.buf[self.off + 4..self.off + self.size()])
		}

		pub fn entries<'b>(&'b mut self) -> EntryIter<'b, 'a> {
			EntryIter(self, self.off + 4)
		}
		
		pub fn entry<'b>(&'b mut self, key: &'b str) -> Entry<'b, 'a> {
			match self.find(key) {
				Some(entry) => Entry::Occupied(OccupiedEntry {
					key_off: entry.key_off,
					val_off: entry.val_off,
					doc:     self,
				}),
				None => Entry::Vacant(VacantEntry { doc: self, key })
			}
		}
		
		pub fn contains<'b, T: BsonValue<'b>>(&mut self, key: &str) -> bool {
			self.find(key).filter(|e| e.is::<T>()).is_some()
		}

		pub fn get<'b: 'a, T: BsonValue<'b>>(&'b mut self, key: &str) -> Option<T> {
			match self.find(key) {
				Some(entry) => if entry.is::<T>() {
					Some(T::read_bson(&entry.doc.buf[entry.val_off..]))
				} else {
					None
				},
				None => None
			}
		}

		pub fn append<'b, T: BsonValue<'b>>(&mut self, key: &str, value: &T) {
			// write size
			
			let off = self.off + self.size() - 1;
			let len = 2 + key.len() + value.size();
			self.set_len(self.size() + len);
			let buf_len = self.buf.len();
			self.buf.resize(buf_len + len, 0);
			self.buf.copy_within(off..buf_len, off + len);
			
			// write entry
			
			self.buf[off] = T::ID;
			self.buf[off + 1..off + 1 + key.len()].copy_from_slice(key.as_bytes());
			self.buf[off + 1 + key.len()] = 0;
			value.write_bson(&mut self.buf[off + 2 + key.len()..]);
		}
		
		pub fn insert<'b, T: BsonValue<'b>>(&mut self, key: &str, value: &T) -> bool {
			match self.find(key) {
				Some(mut e) => { e.insert(value); true },
				None => { self.append(key, value); false }
			}
		}
		
		pub fn remove(&mut self, key: &str) -> bool {
			self.find(key)
				.map(OccupiedEntry::remove)
				.is_some()
		}

		pub fn clear(&mut self) {
			let len = self.buf.len();
			self.buf.copy_within(self.off + len.., self.off + 5);
			self.buf.truncate(self.buf.len() - len + 5);
			self.set_len(5);
		}
	}
	
	impl<'a> IntoIterator for &'a DocMut<'a> {
		type Item     = <DocRefIter<'a> as Iterator>::Item;
		type IntoIter = DocRefIter<'a>;
		
		fn into_iter(self) -> Self::IntoIter {
			DocRefIter(&self.buf[self.off + 4..self.off + self.size()])
		}
	}
}

pub mod entry {
	use super::*;
	
	#[derive(Debug, PartialEq)]
	pub enum Entry<'a, 'b: 'a> {
		Vacant(VacantEntry<'a, 'b>),
		Occupied(OccupiedEntry<'a, 'b>)
	}
	
	impl<'a, 'b> Entry<'a, 'b> {
		pub fn vacant(self) -> Option<VacantEntry<'a, 'b>> {
			match self {
				Self::Vacant(e) => Some(e),
				_ => None
			}
		}
		
		pub fn occupied(self) -> Option<OccupiedEntry<'a, 'b>> {
			match self {
				Self::Occupied(e) => Some(e),
				_ => None
			}
		}
	}
	
	#[derive(Debug, PartialEq)]
	pub struct VacantEntry<'a, 'b: 'a> {
		pub(super) doc: &'a mut DocMut<'b>,
		pub key: &'a str
	}
	
	impl<'a, 'b> VacantEntry<'a, 'b> {
		pub fn insert<'c, T: BsonValue<'c>>(self, value: &T) -> OccupiedEntry<'a, 'b> {
			self.doc.append(self.key, value);
			OccupiedEntry {
				doc:     self.doc,
				key_off: 0,
				val_off: 0
			}
		}
	}
	
	#[derive(Debug, PartialEq)]
	pub struct OccupiedEntry<'a, 'b: 'a> {
		pub(super) doc:  &'a mut DocMut<'b>,
		pub(super) key_off: usize,
		pub(super) val_off: usize,
	}
	
	impl<'a, 'b> OccupiedEntry<'a, 'b> {
		pub fn key(&self) -> &str {
			std::str::from_utf8(&self.doc.buf[self.key_off + 1
				..self.val_off - 1]).unwrap()
		}
		
		pub fn get<'c, T: BsonValue<'c>>(&'c self) -> Option<T> {
			// feature `bool_to_option`
			//self.is::<T>().then(T::read_bson(&self.doc.buf[self.val_off..]))
			if self.is::<T>() {
				Some(T::read_bson(&self.doc.buf[self.val_off..]))
			} else {
				None
			}
		}
		
		pub fn get_any(&self) -> Bson {
			Bson::read(self.doc.buf[self.key_off], &self.doc.buf[self.val_off..]).unwrap()
		}
		
		#[allow(clippy::comparison_chain)]
		pub fn insert<'c, T: BsonValue<'c>>(&mut self, value: &T) {
			let val_len = self.val_size();
			let doc = &mut *self.doc;
			let Self { key_off, val_off, .. } = *self;
			doc.buf[key_off] = T::ID;
			doc.set_len((doc.size() as isize + value.size() as isize - val_len as isize) as _);
			
			if value.size() < val_len {
				doc.buf.copy_within(val_off + val_len.., val_off + value.size());
				doc.buf.truncate(doc.buf.len() + val_len - value.size());
			} else if value.size() > val_len {
				let old_len = doc.buf.len();
				doc.buf.resize(old_len + value.size() - val_len, 0);
				doc.buf.copy_within(val_off + val_len..old_len, val_off + value.size());
			}
			
			value.write_bson(&mut doc.buf[val_off..]);
		}
		
		pub fn insert_any(&mut self, value: Bson) {
			match value {
				Bson::Unit => self.insert(&()),
				Bson::Bool(v) => self.insert(&v),
				Bson::I32(v) => self.insert(&v),
				Bson::I64(v) => self.insert(&v),
				Bson::F64(v) => self.insert(&v),
				Bson::F128(v) => self.insert(&v),
				Bson::ObjectId(v) => self.insert(&v),
				Bson::Timestamp(v) => self.insert(&v),
				Bson::Document(v) => self.insert(&v),
				Bson::Array(v) => self.insert(&v),
				Bson::String(v) => self.insert(&v),
				Bson::Binary(v) => self.insert(&v),
				Bson::Datetime(v) => self.insert(&v),
				Bson::Regex(v) => self.insert(&v),
				Bson::JavaScript(v) => self.insert(&v),
				Bson::JavaScriptWithScope(v) => self.insert(&v),
			}
		}
		
		pub fn remove(self) {
			let len = self.val_off + self.val_size() - self.key_off;
			self.doc.buf.copy_within(self.key_off + len.., self.key_off);
			self.doc.buf.truncate(self.doc.buf.len() - len);
			self.doc.set_len(self.doc.size() - len);
		}
		
		pub fn is<'c, T: BsonValue<'c>>(&self) -> bool {
			self.doc.buf[self.key_off] == T::ID
		}
		
		fn val_size(&self) -> usize {
			get_value_size(self.doc.buf[self.key_off], &self.doc.buf[self.val_off..])
		}
	}
	
	pub struct EntryIter<'a, 'b: 'a>(pub(super) &'a mut DocMut<'b>, pub(super) usize);
}

#[macro_export]
macro_rules! doc {
	( $( $key:expr => $value:expr ),* ) => {{
		$crate::bson::DocBuilder::new()
		$(
			.append($key, $value)
		)*
			.finish()
	}};
	( $( $value:expr ),* ) => [{
		#[allow(clippy::eval_order_dependence)]
		{
			let mut i = 0;
			$crate::bson::Array($crate::bson::DocBuilder::new()
			$(
				.append({ let key = i.to_string(); i += 1; key }.as_str(), $value)
			)*
				.finish().0)
		}
	}]
}

#[cfg(test)]
mod tests {
	use super::*;
	
	#[test]
	fn test_len() {
		let mut doc = Document::new();
		let mut mutdoc = doc.as_mut();
		assert_eq!(mutdoc.size(), 5);
		mutdoc.insert("test", &123i32);
		assert_eq!(mutdoc.size(), 15);
		mutdoc.insert("test", &321i32);
		assert_eq!(mutdoc.size(), 15);
		mutdoc.remove("test");
		assert_eq!(mutdoc.size(), 5);
	}
	
	#[test]
	fn test_insert() {
		let mut doc = Document::new();
		let mut mutdoc = doc.as_mut();
		mutdoc.insert("test", &123i32);
		assert_eq!(mutdoc.iter().next(), Some(("test", Bson::I32(123))));
	}
	
	#[test]
	fn test_remove() {
		let mut doc = Document::new();
		let mut mutdoc = doc.as_mut();
		mutdoc.insert("test", &123i32);
		mutdoc.remove("test");
		assert_eq!(mutdoc.iter().next(), None);
	}
	
	#[test]
	fn test_entry_vacant() {
		let mut doc = Document::new();
		let mut mutdoc = doc.as_mut();
		assert_eq!(mutdoc.entry("test").occupied(), None);
	}
	
	#[test]
	fn test_entry_occupied() {
		let mut doc = Document::new();
		let mut mutdoc = doc.as_mut();
		mutdoc.insert("test", &123i32);
		assert_eq!(mutdoc.entry("test").vacant(), None);
	}
	
	#[test]
	fn test_iter() {
		let mut doc = Document::new();
		let mut mutdoc = doc.as_mut();
		mutdoc.insert("test1", &123i32);
		mutdoc.insert("test2", &321i32);
		mutdoc.insert("test3", &123i32);
		let mut iter = mutdoc.iter();
		assert_eq!(iter.next(), Some(("test1", Bson::I32(123))));
		assert_eq!(iter.next(), Some(("test2", Bson::I32(321))));
		assert_eq!(iter.next(), Some(("test3", Bson::I32(123))));
		assert_eq!(iter.next(), None);
	}
}