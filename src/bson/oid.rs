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

use std::{time::{SystemTime, UNIX_EPOCH}, sync::{Once, atomic::*}};
use serde::{Serialize, Serializer, Deserialize, Deserializer};
use rand::RngCore;
use super::BsonValue;

static INIT:           Once      = Once::new();
static PROCESS_UNIQUE: AtomicU64 = AtomicU64::new(0);
static COUNTER:        AtomicU32 = AtomicU32::new(0);

/// see https://github.com/mongodb/specifications/blob/master/source/objectid.rst
#[derive(Default, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ObjectId(pub [u8; 12]);

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct OidError;

impl std::fmt::Display for OidError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("Error")
	}
}

impl ObjectId {
	pub fn new() -> Self {
		INIT.call_once(|| {
			COUNTER.store(rand::thread_rng().next_u32() & 0x00FF_FFFF, Ordering::Relaxed);
			PROCESS_UNIQUE.store(rand::thread_rng().next_u64() & 0x0000_00FF_FFFF_FFFF, Ordering::Relaxed);
		});
		
		let counter = match COUNTER.load(Ordering::SeqCst) {
			0x00FF_FFFF => {
				COUNTER.store(0, Ordering::SeqCst);
				0
			},
			_          => COUNTER.fetch_add(1, Ordering::SeqCst)
		};

		let unique = PROCESS_UNIQUE.load(Ordering::SeqCst);

		let ts = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_millis();
		
		Self([
			(ts >> 0x18 & 0xFF) as u8,
			(ts >> 0x10 & 0xFF) as u8,
			(ts >> 0x8 & 0xFF) as u8,
			(ts & 0xFF) as u8,
			(unique >> 0x20 & 0xFF) as u8,
			(unique >> 0x18 & 0xFF) as u8,
			(unique >> 0x10 & 0xFF) as u8,
			(unique >> 0x8 & 0xFF) as u8,
			(unique & 0xFF) as u8,
			(counter >> 0x10 & 0xFF) as u8,
			(counter >> 0x8 & 0xFF) as u8,
			(counter & 0xFF) as u8,
		])
	}
}

#[derive(Serialize, Deserialize)]
struct ObjectIdSerde<'a> {
	#[serde(rename = "$oid")]
	#[serde(with = "serde_bytes")]
	oid: &'a [u8]
}

#[derive(Serialize, Deserialize)]
struct ObjectIdSerdeJson<'a> {
	#[serde(rename = "$oid")]
	oid: &'a str
}

impl Serialize for ObjectId {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where
		S: Serializer {
		if serializer.is_human_readable() {
			ObjectIdSerdeJson { oid: &self.to_string() }.serialize(serializer)
		} else {
			ObjectIdSerde { oid: &self.0 }.serialize(serializer)
		}
	}
}

impl<'de> Deserialize<'de> for ObjectId {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error> where
		D: Deserializer<'de> {
		use std::str::FromStr;
		if deserializer.is_human_readable() {
			ObjectIdSerdeJson::deserialize(deserializer)
				.map(|v| Self::from_str(v.oid).unwrap())
		} else {
			ObjectIdSerde::deserialize(deserializer).map(|v| Self({
				let mut oid = [0u8; 12];
				oid.copy_from_slice(&v.oid[..12]);
				oid
			}))
		}
	}
}

impl std::str::FromStr for ObjectId {
	type Err = OidError;
	
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s.len() != 24 { return Err(OidError) }
		
		fn parse_digit(ch: char) -> Result<u8, OidError> {
			Ok(match ch {
				'0' => 0x0, '1' => 0x1, '2' => 0x2, '3' => 0x3, '4' => 0x4,
				'5' => 0x5, '6' => 0x6, '7' => 0x7, '8' => 0x8, '9' => 0x9,
				'a' | 'A' => 0xA, 'b' | 'B' => 0xB, 'c' | 'C' => 0xC,
				'd' | 'D' => 0xD, 'e' | 'E' => 0xE, 'f' | 'F' => 0xF,
				_ => return Err(OidError)
			})
		}
		
		let mut id = [0u8; 12];
		let mut iter = s.chars();
		
		for id in &mut id {
			*id = (iter.next().ok_or(OidError).and_then(parse_digit)? << 4)
				| iter.next().ok_or(OidError).and_then(parse_digit)?;
		}
		Ok(Self(id))
	}
}

impl std::fmt::Debug for ObjectId {
	fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
		f.debug_tuple("ObjectId")
			.field(&self.to_string())
			.finish()
	}
}

impl std::fmt::Display for ObjectId {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let mut buf = [0; 24];
		crate::common::bytes_to_hex(&self.0, &mut buf);
		f.write_str(std::str::from_utf8(&buf).unwrap())
	}
}

impl BsonValue<'_> for ObjectId {
	const ID: u8 = 0x07;

	fn size(&self) -> usize {
		12
	}


	fn write_bson(&self, buf: &mut [u8]) {
		buf[..12].copy_from_slice(&self.0)
	}

	fn read_bson(buf: &[u8]) -> Self {
		Self([
			buf[0], buf[1], buf[2], buf[3], buf[4], buf[5],
			buf[6], buf[7], buf[8], buf[9], buf[10], buf[11]
		])
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::str::FromStr;
	
	#[test]
	fn new() {
		let id = ObjectId::new();
		let id2 = ObjectId::new();
		assert_eq!(id.0[11] + 1, id2.0[11]);
	}
	
	#[test]
	fn parse() {
		assert_eq!(ObjectId::from_str("0123456789ABCDEF01234567"),
				   Ok(ObjectId([0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
					   0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67])))
	}
	
	#[test]
	fn fmt() {
		assert_eq!(ObjectId([0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
			0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67]).to_string(),
				   "0123456789ABCDEF01234567")
	}
}