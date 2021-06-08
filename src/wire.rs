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

#![allow(clippy::float_cmp)]

use {
	crate::{*, bson::{Document, UtcDateTime}},
	std::{io::{self, Read, Write}, net::TcpStream, collections::HashMap, mem::size_of},
	serde::{Serialize, Deserialize}
};

pub const MIN_WIRE_VERSION: i32 = 6;
pub const MAX_WIRE_VERSION: i32 = 8;
pub const SUPPORTED_COMPRESSORS: [Compressor; 1] = [Compressor::Zstd];

#[derive(Debug)]
pub enum InvalidReplyError {
	/// The opcode was not MSG
	OpCode,
	/// Response_to did not match request_id
	ResponseTo,
	/// Payload type was not 0
	PayloadType,
	ErrorCodeNotPresent,
	/// Invalid compressor
	Compression
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum OpCode {
	Compressed   = 2012,
	Msg          = 2013
}

#[repr(packed)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Header {
	pub message_length: u32,
	pub request_id:     i32,
	pub response_to:    i32,
	pub op_code:        OpCode
}

impl Header {
	fn read(reader: &mut impl io::Read) -> io::Result<Self> {
		let mut buf = [0u8; 16];
		reader.read_exact(&mut buf)?;
		Self::copy_from_slice(&buf)
	}

	fn copy_to_slice(self, slice: &mut [u8]) {
		slice[0..4].copy_from_slice(&self.message_length.to_le_bytes());
		slice[4..8].copy_from_slice(&self.request_id.to_le_bytes());
		slice[8..12].copy_from_slice(&self.response_to.to_le_bytes());
		slice[12..16].copy_from_slice(&(self.op_code as i32).to_le_bytes());
	}

	fn copy_from_slice(buf: &[u8]) -> io::Result<Self> {
		Ok(Self {
			message_length: u32::from_le_bytes([buf[0], buf[1], buf[2],  buf[3]]),
			request_id:     i32::from_le_bytes([buf[4], buf[5], buf[6],  buf[7]]),
			response_to:    i32::from_le_bytes([buf[8], buf[9], buf[10],  buf[11]]),
			op_code:        match u32::from_le_bytes([buf[12], buf[13], buf[14],  buf[15]]) {
				2012 => OpCode::Compressed,
				2013 => OpCode::Msg,
				_    => return Err(io::Error::new(
					io::ErrorKind::InvalidData, "invalid op code"))
			}
		})
	}
}

struct CompressionData {
	original_opcode:   OpCode,
	uncompressed_size: u32,
	compressor_id:     Compressor
}

impl CompressionData {
	fn read(reader: &mut impl io::Read) -> io::Result<Self> {
		let mut buf = [0u8; 16];
		reader.read_exact(&mut buf)?;
		Ok(Self {
			original_opcode:   match u32::from_le_bytes([buf[0], buf[1], buf[2],  buf[3]]) {
				2012 => OpCode::Compressed,
				2013 => OpCode::Msg,
				_    => return Err(io::Error::new(
					io::ErrorKind::InvalidData, "invalid op code"))
			},
			uncompressed_size: u32::from_le_bytes([buf[4], buf[5], buf[6],  buf[7]]),
			compressor_id:     match buf[8] {
				0 => Compressor::Noop,
				1 => Compressor::Snappy,
				2 => Compressor::Zlib,
				3 => Compressor::Zstd,
				_ => return Err(io::Error::new(
					io::ErrorKind::InvalidData, "invalid compressor"))
			}
		})
	}

	fn copy_to_slice(self, slice: &mut [u8]) {
		slice[0..4].copy_from_slice(&(self.original_opcode as u32).to_le_bytes());
		slice[4..8].copy_from_slice(&self.uncompressed_size.to_le_bytes());
		slice[8] = self.compressor_id as _;
	}
}

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Compressor {
	Noop   = 0,
	Snappy = 1,
	Zlib   = 2,
	Zstd   = 3
}

impl std::str::FromStr for Compressor {
	type Err = ();
	
	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		Ok(match s {
			"noop"   => Self::Noop,
			"snappy" => Self::Snappy,
			"zlib"   => Self::Zlib,
			"zstd"   => Self::Zstd,
			_ => return Err(())
		})
	}
}

impl Serialize for Compressor {
	fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> where
		S: serde::Serializer {
		serializer.serialize_str(match self {
			Self::Noop   => "noop",
			Self::Snappy => "snappy",
			Self::Zlib   => "zlib",
			Self::Zstd   => "zstd"
		})
	}
}

impl<'de> Deserialize<'de> for Compressor {
	fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error> where
		D: serde::Deserializer<'de> {
		Ok(<&str>::deserialize(deserializer)?.parse().unwrap_or(Self::Noop))
	}
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HandshakeRequest<'a> {
	is_master:   isize,
	#[serde(skip_serializing_if = "Option::is_none")]
	client:      Option<HandshakeRequestClient<'a>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	compression: Option<&'a [Compressor]>,
	#[serde(rename = "$db")]
	db:          &'a str
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HandshakeRequestClient<'a> {
	#[serde(skip_serializing_if = "Option::is_none")]
	application: Option<HandshakeRequestClientApplication<'a>>,
	driver:      HandshakeRequestClientDriver<'a>,
	os:          HandshakeRequestClientOs<'a>,
	#[serde(skip_serializing_if = "Option::is_none")]
	platform:    Option<&'a str>
}

impl Default for HandshakeRequestClient<'_> {
	fn default() -> Self {
		Self {
			application: None,
			driver: HandshakeRequestClientDriver {
				name:    crate::DRIVER_NAME,
				version: env!("CARGO_PKG_VERSION")
			},
			os: HandshakeRequestClientOs {
				r#type:       std::env::consts::OS,
				name:         None,
				architecture: Some(std::env::consts::ARCH),
				version:      None
			},
			platform: None
		}
	}
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HandshakeRequestClientApplication<'a> {
	name: &'a str
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HandshakeRequestClientDriver<'a> {
	name:    &'a str,
	version: &'a str
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HandshakeRequestClientOs<'a> {
	#[serde(rename = "type")]
	r#type:       &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	name:         Option<&'a str>,
	#[serde(skip_serializing_if = "Option::is_none")]
	architecture: Option<&'a str>,
	#[serde(skip_serializing_if = "Option::is_none")]
	version:      Option<&'a str>
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeReply {
	#[serde(flatten)]
	pub generic:                         GenericReply,
	pub ismaster:                        bool,
	pub max_bson_object_size:            i32,
	pub max_message_size_bytes:          i32,
	pub max_write_batch_size:            i32,
	pub local_time:                      UtcDateTime,
	pub logical_session_timeout_minutes: i32,
	pub min_wire_version:                i32,
	pub max_wire_version:                i32,
	pub read_only:                       bool,
	pub compression:                     Option<Vec<Compressor>>,
	pub sasl_supported_mechs:            Option<Vec<String>>,
	// sharded instances
	pub msg:                             Option<String>,
	// replica sets
	pub set_name:                        Option<String>,
	pub set_version:                     Option<i32>,
	pub secondary:                       Option<bool>,
	pub hosts:                           Option<Vec<String>>,
	pub passives:                        Option<Vec<String>>,
	pub arbiters:                        Option<Vec<String>>,
	pub primary:                         Option<String>,
	pub arbiter_only:                    Option<bool>,
	pub passive:                         Option<bool>,
	pub hidden:                          Option<bool>,
	pub tags:                            Option<HashMap<String, String>>,
	pub me:                              Option<String>,
	pub election_id:                     Option<ObjectId>,
	pub last_write:                      Option<LastWrite>,
	pub isreplicaset:                    Option<bool>
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastWrite {
	pub op_time:             ObjectId,
	pub last_write_date:     UtcDateTime,
	pub majority_op_time:    ObjectId,
	pub majority_write_date: UtcDateTime
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenericReply {
	pub ok:        f64,
	pub errmsg:    Option<String>,
	pub code:      Option<i32>,
	pub code_name: Option<String>
}

impl Into<Result<()>> for GenericReply {
	fn into(self) -> Result<()> {
		if self.ok == 1f64 {
			Ok(())
		} else {
			Err(Error::Operation((self.code.unwrap() as i16).into(), self.errmsg.unwrap()))
		}
	}
}

pub trait Wire: Read + Write + Sized {
	/// Performs a handshake with a mongodb server.
	///
	/// see https://github.com/mongodb/specifications/blob/master/source/mongodb-handshake/handshake.rst
	/// https://github.com/mongodb/specifications/blob/master/source/auth/auth.rst
	/// https://github.com/mongodb/specifications/blob/master/source/compression/OP_COMPRESSED.rst
	fn handshake(
		&mut self,
		options:  &ClientOptions,
		metadata: bool,
		auth:     bool
	) -> Result<HandshakeReply> {
		self.send(0, None, &Document::from(&HandshakeRequest {
			db:        "local",
			is_master: 1,
			client:    metadata.then(|| HandshakeRequestClient {
				application: options.appname.as_ref().map(|s| HandshakeRequestClientApplication {
					name: s.as_str()
				}),
				..HandshakeRequestClient::default()
			}),
			compression: options.compressors.as_deref()
		})?, &[])?;

		let reply: HandshakeReply = self.recv(0)?.into()?;
		
		if reply.generic.ok != 1f64 {
			return Err(reply.generic.into())
		}

		if cfg!(feature = "auth") && auth && options.credential.mechanism.is_some() {
			auth::auth(self, 1, &options.credential)?;
		}

		Ok(reply)
	}

	/// Sends a request with the given request id.
	fn send(
		&mut self,
		request_id:  i32,
		compression: Option<Compressor>,
		doc:         &Document,
		sequences:   &[&[(&str, &[Document])]]
	) -> Result<()> {
		let mut buf = vec![0u8; size_of::<Header>() + 5];  // flag bits, payload type
		buf.write_all(&doc.0)?;
		
		for sequence in sequences {
			for (identifier, documents) in *sequence {
				buf.write_all(str::as_bytes(identifier))?;
				buf.write_all(&[0])?;
				buf.write_all(&(documents.iter()
					.map(|doc| doc.0.len())
					.sum::<usize>() as u32).to_le_bytes())?;

				for doc in *documents {
					buf.write_all(&doc.0)?;
				}
			}
		}
		
		match compression {
			None => {
				Header {
					message_length: buf.len() as _,
					request_id,
					response_to:    0,
					op_code:        OpCode::Msg
				}.copy_to_slice(&mut buf[..size_of::<Header>()]);

				self.write_all(&buf)?;
			}
			Some(comp) => {
				let buf = &buf[size_of::<Header>()..];
				let mut buf0 = vec![0u8; size_of::<Header>() + size_of::<CompressionData>()];

				match comp {
					Compressor::Noop => buf0.write_all(buf)?,
					#[cfg(feature = "compress")]
					Compressor::Zstd => {
						let mut encoder = zstd::Encoder::new(&mut buf0, 0)?;
						encoder.write_all(buf)?;
						encoder.finish()?;
					}
					compressor => unimplemented!("unsupported compressor: {:?}", compressor)
				}

				Header {
					message_length: buf0.len() as _,
					request_id,
					response_to:    0,
					op_code:        OpCode::Compressed
				}.copy_to_slice(&mut buf0[..size_of::<Header>()]);

				CompressionData {
					original_opcode:   OpCode::Msg,
					uncompressed_size: buf.len() as _,
					compressor_id:     comp
				}.copy_to_slice(&mut buf0[size_of::<Header>()
					..size_of::<Header>() + size_of::<CompressionData>()]);

				self.write_all(&buf0)?;
			}
		}
		
		Ok(())
	}

	/// Tries to receive a response for the given request id, returning a bson document on success.
	fn recv(&mut self, request_id: i32) -> Result<Document> {
		let mut header = Header::read(self)?;
		let mut buf;

		if { header.op_code } == OpCode::Compressed {
			let data = CompressionData::read(self)?;
			header.op_code = data.original_opcode;
			header.message_length = data.uncompressed_size;

			buf = vec![0u8; header.message_length as usize];
			match data.compressor_id {
				Compressor::Noop => self.read_exact(buf.as_mut_slice())?,
				#[cfg(feature = "compress")]
				Compressor::Zstd => zstd::stream::read::Decoder::new(self)?.read_exact(buf.as_mut_slice())?,
				_ => return Err(Error::InvalidReply(InvalidReplyError::Compression))
			}
		} else {
			header.message_length -= size_of::<Header>() as u32;
			buf = vec![0u8; header.message_length as usize];
			self.read_exact(buf.as_mut_slice())?;
		}

		let _flag_bits = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
		let payload_type = buf[4];

		if { header.op_code } != OpCode::Msg {
			Err(Error::InvalidReply(InvalidReplyError::OpCode))
		} else if { header.response_to } != request_id {
			Err(Error::InvalidReply(InvalidReplyError::ResponseTo))
		} else if payload_type != 0 {
			Err(Error::InvalidReply(InvalidReplyError::PayloadType))
		} else {
			Ok(Document::copy_from_slice(&buf[5..]))
		}
	}

	fn send_recv(
		&mut self,
		request_id:  i32,
		compression: Option<Compressor>,
		doc:         &Document,
		sequences:   &[&[(&str, &[Document])]]
	) -> Result<Document> {
		self.send(request_id, compression, doc, sequences)?;
		self.recv(request_id)
	}
}

impl<T: Read + Write> Wire for T {}

#[allow(clippy::large_enum_variant)]
pub enum Stream {
	Tcp(TcpStream, Option<Compressor>),
	#[cfg(feature = "tls")]
	Tls(rustls::StreamOwned<rustls::ClientSession, TcpStream>, Option<Compressor>),
	Empty
}

impl std::fmt::Debug for Stream {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.write_str(match self {
			Self::Tcp(..) => "Stream::Tcp(...)",
			#[cfg(feature = "tls")]
			Self::Tls(..) => "Stream::Tls(...)",
			Self::Empty   => "Stream::Empty"
		})
	}
}

impl Stream {
	/// Connects to a mongodb server with the given options.
	pub fn connect(
		host_name:  &str,
		port:       u16,
		options:    &ClientOptions,
		compressor: Option<Compressor>
	) -> Result<Self> {
		#[cfg(not(feature = "tls"))]
		return Ok(Stream::Tcp(TcpStream::connect((host_name, port))?, compressor));
		
		#[cfg(feature = "tls")]
		Ok(match &options.tls_config {
			None => Stream::Tcp(TcpStream::connect((host_name, port))?, compressor),
			Some(options) => Stream::Tls(rustls::StreamOwned::new(
				rustls::ClientSession::new(options,
				webpki::DNSNameRef::try_from_ascii_str(host_name)?),
				TcpStream::connect((host_name, port))?
			), compressor)
		})
	}

	/// Performs a handshake with a mongodb server.
	pub fn handshake(
		&mut self,
		options:  &ClientOptions,
		metadata: bool,
		auth:     bool
	) -> Result<HandshakeReply> {
		match self {
			Self::Tcp(stream, ..) => stream.handshake(options, metadata, auth),
			#[cfg(feature = "tls")]
			Self::Tls(stream, ..) => stream.handshake(options, metadata, auth),
			Self::Empty => panic!()
		}
	}

	/// Sends a request with the given request id.
	pub fn send(
		&mut self,
		request_id: i32,
		doc:        &Document,
		sequences:  &[&[(&str, &[Document])]]
	) -> Result<()> {
		match self {
			Self::Tcp(stream, compression) => stream.send(request_id, *compression, doc, sequences),
			#[cfg(feature = "tls")]
			Self::Tls(stream, compression) => stream.send(request_id, *compression, doc, sequences),
			Self::Empty => panic!()
		}
	}

	/// Tries to receive a response for the given request id, returning a bson document on success.
	pub fn revc(&mut self, request_id: i32) -> Result<Document> {
		match self {
			Self::Tcp(stream, ..) => stream.recv(request_id),
			#[cfg(feature = "tls")]
			Self::Tls(stream, ..) => stream.recv(request_id),
			Self::Empty => panic!()
		}
	}

	pub fn send_recv(
		&mut self,
		request_id: i32,
		doc:        &Document,
		sequences:  &[&[(&str, &[Document])]]
	) -> Result<Document> {
		match self {
			Self::Tcp(stream, compression) => stream.send_recv(request_id, *compression, doc, sequences),
			#[cfg(feature = "tls")]
			Self::Tls(stream, compression) => stream.send_recv(request_id, *compression, doc, sequences),
			Self::Empty => panic!()
		}
	}
}