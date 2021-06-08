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

#![warn(clippy::all)]
#![allow(clippy::from_over_into)]
#![forbid(unsafe_code)]

use {
	self::{wire::GenericReply, topology::TopologyDescription, common::{ChangeStreamStage, Result}},
	std::{sync::{Arc, RwLock, atomic::{AtomicUsize, AtomicBool, Ordering}}},
	serde::{Serialize, Deserialize, de::DeserializeOwned}
};

pub use self::{db::*, coll::*, cursor::*, bson::*, utils::*, common::*};

pub mod db;
pub mod coll;
pub mod cursor;
pub mod wire;
pub mod topology;
pub mod apm;
#[cfg(feature = "auth")]
pub mod auth;
pub mod common;
pub mod bson;
pub mod utils;

pub static DRIVER_NAME: &str = "mongo-rust-driver";

#[derive(Debug, Clone)]
pub struct Client(Arc<ClientInner>);

pub struct ClientInner {
	pub options:         ClientOptions,
	pub read_preference: ReadPreference,
	pub read_concern:    Option<ReadConcern>,
	pub write_concern:   Option<WriteConcern>,
	topology:            RwLock<TopologyDescription>,
	request_id:          AtomicUsize,
	operation_id:        AtomicUsize,
	listeners_set:       AtomicBool,
	listeners:           RwLock<Vec<apm::EventListener>>
}

impl std::ops::Deref for Client {
	type Target = ClientInner;
	
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl std::fmt::Debug for ClientInner {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.debug_struct("ClientInner")
			.field("options", &self.options)
			.field("read_preference", &self.read_preference)
			.field("read_concern", &self.read_concern)
			.field("write_concern", &self.write_concern)
			.field("topology", &self.topology)
			.field("request_id", &self.request_id)
			.field("listeners_set", &self.listeners_set)
			.finish()
	}
}

impl Client {
	pub fn new(builder: ClientOptions) -> Result<Self> {
		let self_ = Self(Arc::new(ClientInner {
			read_preference: builder.read_preference.clone(),
			read_concern:    builder.read_concern,
			write_concern:   builder.write_concern,
			topology:        RwLock::default(),
			options:         builder,
			request_id:      AtomicUsize::new(1),
			operation_id:    AtomicUsize::new(1),
			listeners_set:   AtomicBool::new(false),
			listeners:       RwLock::new(Vec::new())
		}));
		self_.topology.write()?.init(&self_)?;
		Ok(self_)
	}
	
	pub fn acquire_stream(&self, read: Option<&ReadPreference>) -> Result<topology::Connection> {
		let start = std::time::SystemTime::now();
		loop {
			return match self.topology.read()?.acquire_stream(self, read) {
				Ok(stream) => Ok(stream),
				Err(e) => if start.elapsed().unwrap() > self.options.server_selection_config.server_selection_timeout {
					Err(e)
				} else {
					continue;
				}
			}
		}
	}
	
	pub fn run_command<R: DeserializeOwned>(
		&self,
		command:       &impl Serialize,
		read:          Option<&ReadPreference>,
		mut retryable: bool
	) -> Result<R> {
		let doc = Document::from(command)?;
		let command_name = doc.into_iter().next()
			.map_or(apm::Command::Unknown, |(k, _)| k.into());
		let request_id = self.request_id.fetch_add(1, Ordering::SeqCst);
		let operation_id = self.operation_id.fetch_add(1, Ordering::SeqCst);
		
		loop {
			let ts = std::time::SystemTime::now();
			let mut stream = self.acquire_stream(read)?;
			
			self.dispatch_event(apm::Event::CommandStarted {
				command:       &doc,
				database_name: {
					#[derive(Deserialize)]
					struct DbCmd<'a> { #[serde(rename = "$db")] db: &'a str }
					doc.deserialize::<DbCmd>().ok().map(|cmd| cmd.db)
				},
				command_name,
				request_id,
				operation_id,
				connection_id: stream.id
			})?;
			
			#[inline]
			#[allow(clippy::float_cmp)]
			fn run_command_inner(
				stream:     &mut topology::Connection,
				id:         i32,
				doc:        &bson::Document
			) -> Result<bson::Document> {
				stream.send(id, doc, &[])?;
				let doc = stream.revc(id)?;
				match doc.deserialize::<GenericReply>() {
					Ok(reply) if reply.ok == 1f64 => Ok(doc),
					Ok(reply) => Err(reply.into()),
					Err(err) => Err(Error::BsonDecode(err)),
				}
			}
			
			return match run_command_inner(&mut stream, request_id as i32, &doc) {
				Err(err) => {
					self.dispatch_event(apm::Event::CommandFailed {
						duration:      ts.elapsed().unwrap(),
						failure:       &err,
						command_name,
						request_id,
						operation_id,
						connection_id: stream.id
					})?;
					
					match err {
						Error::Io(_)
						| Error::Operation(MongoError::PrimarySteppedDown, _)
						| Error::Operation(MongoError::ShutdownInProgress, _)
						| Error::Operation(MongoError::HostNotFound, _)
						| Error::Operation(MongoError::HostUnreachable, _)
						| Error::Operation(MongoError::NetworkTimeout, _)
						if retryable && match read {
							Some(_) => self.options.retry_reads,
							None => self.options.retry_writes
						} => {
							retryable = false;
							continue
						},
						err => Err(err)
					}
				}
				Ok(doc) => {
					self.dispatch_event(apm::Event::CommandSucceeded {
						duration:      ts.elapsed().unwrap(),
						reply:         &doc,
						command_name,
						request_id,
						operation_id,
						connection_id: stream.id
					})?;
					Ok(doc.into()?)
				}
			}
		}
	}
	
	pub fn add_event_listener(&self, listener: impl Fn(&Self, &apm::Event) + Send + Sync + 'static) -> Result<()> {
		self.listeners_set.store(true, Ordering::SeqCst);
		self.listeners.write()?.push(Box::new(listener));
		Ok(())
	}
	
	pub fn dispatch_event(&self, event: apm::Event) -> Result<()> {
		if !self.listeners_set.load(Ordering::SeqCst) { return Ok(()); }
		for listener in self.listeners.read()?.iter() {
			listener(self, &event)
		}
		Ok(())
	}
	
	pub fn db(&self, name: &str) -> DataBase {
		self.db_with(name, None, None, None)
	}
	
	pub fn db_with(
		&self,
		name:            &str,
		read_preference: Option<ReadPreference>,
		read_concern:    Option<ReadConcern>,
		write_concern:   Option<WriteConcern>
	) -> DataBase {
		DataBase(Arc::new(DataBaseInner {
			client:          self.clone(),
			name:            name.to_string(),
			read_preference: read_preference.unwrap_or_else(|| self.read_preference.clone()),
			read_concern:    read_concern.or(self.read_concern),
			write_concern:   write_concern.or(self.write_concern)
		}))
	}
	
	/// Allows a client to observe all changes in a cluster.
	/// Excludes system collections.
	/// Excludes the "config", "local", and "admin" databases.
	pub fn watch(
		&self,
		pipeline: &[bson::Document],
		options:  Option<ChangeStreamOptions>
	) -> Result<ChangeStream> {
		self.db("admin").aggregate(
			{
				let mut pipeline = pipeline.to_vec();
				pipeline.insert(0, Document::from(&ChangeStreamStage {
					change_stream: ChangeStreamOptions {
						all_changes_for_cluster: Some(true),
						..options.as_ref().map(Clone::clone).unwrap_or_default()
					}
				})?);
				pipeline
			}.as_slice(),
			options.map(|options| AggregateOptions {
				batch_size:                 options.batch_size,
				collation:                  options.collation,
				max_await_time_ms:          options.max_await_time_ms,
				..AggregateOptions::default()
			})
		).map(ChangeStream)
	}
}

#[cfg(test)]
mod tests {
	use {super::*, std::str::FromStr};

	//#[test]
	fn test_insert_one() {
		let client = ClientOptions::from_str("mongodb://localhost:27017/?serverSelectionTimeoutMS=500").unwrap()
			.connect().unwrap();

		client.add_event_listener(|_, event| println!("{:?}", event)).unwrap();
		let coll = client.db("test").collection("test");

		coll.drop().unwrap_or(());
		assert!(coll.find(None, None).unwrap().next().is_none());
		coll.insert_one(doc! { "test" => "test" }, None).unwrap();
		assert!(coll.find(None, None).unwrap().next().unwrap().unwrap().into_iter()
					   .collect::<Vec<_>>().contains(&("test", Bson::String("test"))));
	}
}
