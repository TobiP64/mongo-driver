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
	crate::{*, wire::{Stream, HandshakeReply, Compressor}, apm::*, common::DEFAULT_MONGO_PORT},
	std::{
		sync::{Mutex, Arc, Weak, Condvar, RwLock},
		time::{SystemTime, Duration, UNIX_EPOCH},
		convert::identity,
		collections::HashMap,
	},
	rand::Rng
};

/// see https://github.com/mongodb/specifications/blob/master/source/server-discovery-and-monitoring/server-discovery-and-monitoring.rst#topologydescription
#[derive(Debug, Default)]
pub struct TopologyDescription {
	pub servers:                         HashMap<String, Server>,
	pub r#type:                          TopologyType,
	pub set_name:                        Option<String>,
	pub max_set_version:                 Option<i32>,
	pub max_election_id:                 Option<bson::ObjectId>,
	pub compatibility_error:             Option<CompatibilityError>,
	pub logical_session_timeout_minutes: Option<usize>
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TopologyType {
	Single,
	ReplicaSetNoPrimary,
	ReplicaSetWithPrimary,
	Sharded,
	Unknown
}

impl Default for TopologyType {
	fn default() -> Self {
		Self::Unknown
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ServerSelectionError {
	TopologyTypeUnknown,
	NoMatchingServer,
	CompatibilityError(CompatibilityError)
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CompatibilityError {
	WireVersionToOld,
	WireVersionToNew
}

impl TopologyDescription {

	/// Initializes this topology by connecting to the hosts of the clients options.
	pub fn init(&mut self, client: &Client) -> Result<()> {
		self.r#type = client.options.initial_topology_type.unwrap_or(TopologyType::Unknown);
		self.set_name = client.options.replica_set.clone();
		self.servers = client.options.hosts.iter()
			.map(|address| Server::new(address, client).map(|v| (address.clone(), v)))
			.collect::<Result<_>>()?;
		Ok(())
	}

	/// Acquires a stream. If `read_prefence` is Some, the stream is read-only.
	pub fn acquire_stream(&self, client: &Client, read_preference: Option<&ReadPreference>) -> Result<Connection> {
		if let Some(ReadPreference { mode: ReadPreferenceMode::Primary, max_staleness_seconds, tag_sets }) = read_preference {
			debug_assert!(*max_staleness_seconds == -1);
			debug_assert!(tag_sets.is_empty());
		}
		
		if let Some(err) = self.compatibility_error {
			return Err(Error::ServerSelection(ServerSelectionError::CompatibilityError(err)))
		}
		
		match self.r#type {
			TopologyType::Unknown => return Err(Error::ServerSelection(ServerSelectionError::TopologyTypeUnknown)),
			TopologyType::Single  => return self.servers.values().next().unwrap().acquire_stream(client),
			TopologyType::Sharded => self.servers.values()
				.map(|server| server.acquire_stream(client))
				.find(Result::is_ok),
			_ if read_preference.is_none() => self.choose_server(client, ServerType::RSPrimary, read_preference),
			_ => match read_preference.unwrap().mode {
				ReadPreferenceMode::Primary            => self.choose_server(client, ServerType::RSPrimary, read_preference),
				ReadPreferenceMode::PrimaryPreferred   => self.choose_server(client, ServerType::RSPrimary, None)
						.or_else(|| self.choose_server(client, ServerType::RSSecondary, read_preference)),
				ReadPreferenceMode::Secondary          => self.choose_server(client, ServerType::RSSecondary, read_preference),
				ReadPreferenceMode::SecondaryPreferred => self.choose_server(client, ServerType::RSSecondary, read_preference)
						.or_else(|| self.choose_server(client, ServerType::RSPrimary, None)),
				ReadPreferenceMode::Nearest            => self.servers.values()
					.filter(|server| server.description.read().unwrap().r#type != ServerType::RSOther)
					.map(|server| server.acquire_stream(client))
					.find(Result::is_ok)
			}
		}.ok_or(Error::ServerSelection(ServerSelectionError::NoMatchingServer))
			.and_then(identity)
	}
	
	fn choose_server(
		&self,
		client:          &Client,
		r#type:          ServerType,
		read_preference: Option<&ReadPreference>
	) -> Option<Result<Connection>> {
		self.servers.values()
			.filter(|server| {
				let desc = server.description.read().unwrap();
				desc.r#type == r#type && match read_preference {
					Some(read_pref) => (match desc.r#type {
						ServerType::RSSecondary | ServerType::RSArbiter | ServerType::RSGhost if read_pref.max_staleness_seconds >= 0 => (match self.r#type {
							TopologyType::ReplicaSetWithPrimary => 0, // TODO staleness
							TopologyType::ReplicaSetNoPrimary => 0,
							_ => unreachable!()
						}) <= read_pref.max_staleness_seconds * 1000,
						_ => true
					}) && read_pref.tag_sets.iter()
						.any(|tags| !tags.iter()
							.any(|(k, v)| desc.tags.get(k)
								.filter(|v_| *v_ == v)
								.is_none())),
					_ => true
				}
			})
			.cycle()
			.skip(rand::thread_rng().gen_range(0, self.servers.len()))
			.map(|server| server.acquire_stream(client))
			.find(Result::is_ok)
	}
	
	/// Updates this topology according to
	/// https://github.com/mongodb/specifications/blob/master/source/server-discovery-and-monitoring/server-discovery-and-monitoring.rst
	fn update(&mut self, server: &Server, client: &Client) -> Result<()> {
		let description = server.description.read()?;
		
		if description.min_wire_version > wire::MAX_WIRE_VERSION {
			self.compatibility_error = Some(CompatibilityError::WireVersionToNew);
		} else if description.max_wire_version < wire::MIN_WIRE_VERSION {
			self.compatibility_error = Some(CompatibilityError::WireVersionToOld);
		} else if self.r#type != TopologyType::Single && self.compatibility_error.is_some()
			&& !self.servers.values()
			.filter(|s| s.host_name != server.host_name || s.port != server.port)
			.map(|server| server.description.read().unwrap())
			.any(|description| description.min_wire_version > wire::MAX_WIRE_VERSION
				|| description.max_wire_version < wire::MIN_WIRE_VERSION) {
			self.compatibility_error = None;
		}
		
		match (self.r#type, description.r#type) {
			// no-op
			(TopologyType::Single, _)
			| (_, ServerType::PossiblePrimary)
			| (TopologyType::Unknown, ServerType::Unknown)
			| (TopologyType::Unknown, ServerType::RSGhost)
			| (TopologyType::Sharded, ServerType::Unknown)
			| (TopologyType::Sharded, ServerType::Mongos)
			| (TopologyType::ReplicaSetNoPrimary, ServerType::Unknown)
			| (TopologyType::ReplicaSetNoPrimary, ServerType::RSGhost) => (),
			// remove
			(TopologyType::Sharded, _)
			| (TopologyType::ReplicaSetNoPrimary, ServerType::Standalone)
			| (TopologyType::ReplicaSetNoPrimary, ServerType::Mongos) => {
				self.servers.remove(&description.address);
			}
			(TopologyType::ReplicaSetWithPrimary, ServerType::Standalone)
			| (TopologyType::ReplicaSetWithPrimary, ServerType::Mongos) => {
				self.servers.remove(&description.address);
				self.check_if_has_primary();
			}
			// update topology type
			(TopologyType::Unknown, ServerType::Standalone)  => self.r#type = TopologyType::Single,
			(TopologyType::Unknown, ServerType::Mongos)      => self.r#type = TopologyType::Sharded,
			
			// replica set primary
			// updateRSFromPrimary
			(TopologyType::Unknown, ServerType::RSPrimary)
			| (TopologyType::ReplicaSetNoPrimary, ServerType::RSPrimary)
			| (TopologyType::ReplicaSetWithPrimary, ServerType::RSPrimary) => {
				self.r#type = TopologyType::ReplicaSetWithPrimary;
				
				// update set name
				
				if self.set_name.is_none() {
					self.set_name = description.set_name.clone()
				} else if self.set_name != description.set_name {
					self.servers.remove(&description.address);
					self.check_if_has_primary();
					return Ok(());
				}
				
				// update setVersion and electionId
				
				if let (Some(set_version), Some(election_id)) = (description.set_version, description.election_id) {
					if let (Some(max_set_version), Some(max_election_id)) = (self.max_set_version, self.max_election_id) {
						if max_set_version > set_version || (max_set_version == set_version && max_election_id > election_id) {
							let address = description.address.clone();
							drop(description);
							*server.description.write()? = ServerDescription {
								address,
								..ServerDescription::default()
							};
							self.check_if_has_primary();
							return Ok(());
						}
					}
					self.max_election_id = description.election_id;
				}
				
				if let Some(desc_set_version) = description.set_version.as_ref() {
					match self.max_set_version {
						Some(version) if *desc_set_version > version => self.max_set_version = Some(*desc_set_version),
						None => self.max_set_version = Some(*desc_set_version),
						_ => ()
					}
				}
				
				// invalidate old primary
				
				for server in self.servers.values().filter(|server| {
					let description_ = server.description.read().unwrap();
					description_.r#type == ServerType::RSPrimary && description_.address != description.address
				}) {
					server.description.write()?.r#type = ServerType::Unknown;
				}
				
				// update servers
				
				self.servers.retain(|address, _| description.hosts.contains(address)
					|| description.passives.contains(address)
					|| description.arbiters.contains(address));
				
				description.hosts.iter()
					.chain(&description.passives)
					.chain(&description.arbiters)
					.try_for_each::<_, Result<()>>(|address| {
						if !self.servers.contains_key(address) {
							self.servers.insert(address.clone(), Server::new(address, client)?);
						}
						Ok(())
					})?;
				
				self.check_if_has_primary();
			}
			
			// replica set without primary
			// updateRSWithoutPrimary
			(TopologyType::Unknown, ServerType::RSSecondary)
			| (TopologyType::Unknown, ServerType::RSArbiter)
			| (TopologyType::Unknown, ServerType::RSOther)
			| (TopologyType::ReplicaSetNoPrimary, ServerType::RSSecondary)
			| (TopologyType::ReplicaSetNoPrimary, ServerType::RSArbiter)
			| (TopologyType::ReplicaSetNoPrimary, ServerType::RSOther) => {
				self.r#type = TopologyType::ReplicaSetNoPrimary;
				
				// check set name
				
				if self.set_name.is_none() {
					self.set_name = description.set_name.clone()
				} else if self.set_name != description.set_name {
					self.servers.remove(&description.address);
					return Ok(());
				}
				
				// add new servers
				
				description.hosts.iter()
					.chain(&description.passives)
					.chain(&description.arbiters)
					.try_for_each::<_, Result<()>>(|address| {
						if !self.servers.contains_key(address) {
							self.servers.insert(address.clone(), Server::new(address, client)?);
						}
						Ok(())
					})?;
				
				// mark possibly primary
				
				if let Some(server) = description.primary.as_ref()
					.and_then(|primary| self.servers.get(primary))
					.filter(|server| server.description
						.read().unwrap().r#type == ServerType::Unknown) {
					server.description.write()?.r#type = ServerType::PossiblePrimary;
				}
				
				if description.me.as_ref().unwrap() != &description.address {
					self.servers.remove(&description.address);
				}
			}
			
			// replica set with primary
			// updateRSWithPrimaryFromMember
			(TopologyType::ReplicaSetWithPrimary, ServerType::RSSecondary)
			| (TopologyType::ReplicaSetWithPrimary, ServerType::RSArbiter)
			| (TopologyType::ReplicaSetWithPrimary, ServerType::RSOther) => {
				if self.set_name != description.set_name || &description.address != description.me.as_ref().unwrap() {
					self.servers.remove(&description.address);
					self.check_if_has_primary();
					return Ok(());
				}
				
				self.check_if_has_primary();
				
				// mark possible primary
				
				if let Some(server) = description.primary.as_ref()
					.and_then(|primary| self.servers.get(primary))
					.filter(|server| server.description
						.read().unwrap().r#type == ServerType::Unknown) {
					server.description.write()?.r#type = ServerType::PossiblePrimary;
				}
			}
			
			(TopologyType::ReplicaSetWithPrimary, ServerType::Unknown)
			| (TopologyType::ReplicaSetWithPrimary, ServerType::RSGhost) =>
				self.check_if_has_primary()
		}
		Ok(())
	}
	
	/// Checks if this topology has a primary, setting the type to `no primary` if none has been found.
	fn check_if_has_primary(&mut self) {
		if !self.servers.values()
			.any(|server| server.description.read().unwrap().r#type == ServerType::RSPrimary)
		{
			self.r#type = TopologyType::ReplicaSetNoPrimary;
		}
	}
}

#[derive(Debug, Clone)]
pub struct Server(Arc<ServerInner>);

#[derive(Debug)]
pub struct ServerInner {
	pub description: RwLock<ServerDescription>,
	host_name:       String,
	port:            u16,
	monitor:         Monitor,
	pool:            Mutex<Pool>,
	wait_lock:       Condvar
}

/// see https://github.com/mongodb/specifications/blob/master/source/server-discovery-and-monitoring/server-discovery-and-monitoring.rst#serverdescription
#[derive(Debug, Clone, Default)]
pub struct ServerDescription {
	pub address:                         String,
	pub r#type:                          ServerType,
	pub rtt:                             Duration,
	pub last_update_time:                Option<SystemTime>,
	// fields from handshake
	pub last_write_date:                 Option<SystemTime>,
	pub op_time:                         Option<ObjectId>,
	pub min_wire_version:                i32,
	pub max_wire_version:                i32,
	pub me:                              Option<String>,
	pub hosts:                           Vec<String>,
	pub passives:                        Vec<String>,
	pub arbiters:                        Vec<String>,
	pub tags:                            std::collections::HashMap<String, String>,
	pub set_name:                        Option<String>,
	pub set_version:                     Option<i32>,
	pub election_id:                     Option<ObjectId>,
	pub primary:                         Option<String>,
	pub logical_session_timeout_minutes: Option<i32>,
	// not defined in the spec
	pub compression:                     Vec<Compressor>
}

impl ServerDescription {
	fn update(&mut self, reply: Result<HandshakeReply>, ts: SystemTime) -> Result<bool> {
		let reply = match reply {
			Ok(reply) => reply,
			Err(e) => {
				self.r#type           = ServerType::Unknown;
				self.last_update_time = Some(SystemTime::now());
				return Err(e);
			}
		};
		
		let r#type = match &reply {
			HandshakeReply { msg: Some(msg), .. } if msg == "isdbgrid" => ServerType::Mongos,
			HandshakeReply { set_name: Some(_), ismaster:     true,       .. } => ServerType::RSPrimary,
			HandshakeReply { set_name: Some(_), secondary:    Some(true), .. } => ServerType::RSSecondary,
			HandshakeReply { set_name: Some(_), arbiter_only: Some(true), .. } => ServerType::RSArbiter,
			HandshakeReply { set_name: Some(_),                           .. } => ServerType::RSOther,
			HandshakeReply { isreplicaset: Some(true),                    .. } => ServerType::RSGhost,
			_                                                                  => ServerType::Standalone
		};
		
		Ok(if self.r#type == r#type
			&& self.min_wire_version == reply.min_wire_version
			&& self.max_wire_version == reply.min_wire_version
			&& self.me == reply.me
			&& if let Some(hosts) = &reply.hosts { &self.hosts == hosts } else { self.hosts.is_empty() }
			&& if let Some(passives) = &reply.passives { &self.passives == passives } else { self.passives.is_empty() }
			&& if let Some(arbiters) = &reply.arbiters { &self.arbiters == arbiters } else { self.arbiters.is_empty() }
			&& if let Some(tags) = &reply.tags { &self.tags == tags } else { self.tags.is_empty() }
			&& self.set_name == reply.set_name
			&& self.set_version == reply.set_version
			&& self.election_id == reply.election_id
			&& self.primary == reply.primary
			&& self.logical_session_timeout_minutes == Some(reply.logical_session_timeout_minutes) {
			self.r#type           = r#type;
			self.rtt              = ts.elapsed().unwrap();
			self.last_update_time = Some(SystemTime::now());
			self.last_write_date  = reply.last_write.as_ref().map(|v|
				UNIX_EPOCH + Duration::from_millis(v.last_write_date.0 as _));
			self.op_time          = reply.last_write.as_ref().map(|v| v.op_time);
			false
		} else {
			*self = ServerDescription {
				address:                         self.address.clone(),
				r#type,
				rtt:                             ts.elapsed().unwrap(),
				last_update_time:                Some(SystemTime::now()),
				last_write_date:                 reply.last_write.as_ref().map(|v|
					UNIX_EPOCH + Duration::from_millis(v.last_write_date.0 as _)),
				op_time:                         reply.last_write.as_ref().map(|v| v.op_time),
				min_wire_version:                reply.min_wire_version,
				max_wire_version:                reply.max_wire_version,
				me:                              reply.me,
				hosts:                           reply.hosts.unwrap_or_default(),
				passives:                        reply.passives.unwrap_or_default(),
				arbiters:                        reply.arbiters.unwrap_or_default(),
				tags:                            reply.tags.unwrap_or_default(),
				set_name:                        reply.set_name,
				set_version:                     reply.set_version,
				election_id:                     reply.election_id,
				primary:                         reply.primary,
				logical_session_timeout_minutes: Some(reply.logical_session_timeout_minutes),
				compression:                     reply.compression.unwrap_or_default()
			};
			true
		})
	}
}

#[derive(Debug)]
struct Pool {
	len:       usize,
	streams:   Vec<(Stream, usize, SystemTime)>,
	iteration: usize
}

#[derive(Debug)]
struct Monitor {
	client:               Weak<ClientInner>,
	heartbeat_frequency:  Duration,
	dummy_lock:           Mutex<()>,
	condvar:              Condvar,
	update_topology_only: AtomicBool
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ServerType {
	Standalone,
	Mongos,
	PossiblePrimary,
	RSPrimary,
	RSSecondary,
	RSArbiter,
	RSOther,
	RSGhost,
	Unknown
}

impl Default for ServerType {
	fn default() -> Self {
		Self::Unknown
	}
}

impl std::ops::Deref for Server {
	type Target = ServerInner;
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl Server {
	pub fn new(address: &str, client: &Client) -> Result<Self> {
		
		// parse address
		
		let (host_name, port) = if let Some(i) = address.find(':') {
			(address[..i].to_string(), address[i + 1..].parse().map_err(|_|
				 std::io::Error::new(std::io::ErrorKind::Other, "failed to parse server address"))?)
		} else {
			(address.to_string(), DEFAULT_MONGO_PORT)
		};
		
		// init server
		
		let self_ = Self(Arc::new(ServerInner {
			description: RwLock::new(ServerDescription {
				address: address.to_string(),
				..ServerDescription::default()
			}),
			host_name,
			port,
			monitor:   Monitor {
				client:               Arc::downgrade(&client.0),
				heartbeat_frequency:  client.options.server_selection_config.heartbeat_frequency,
				dummy_lock:           Mutex::new(()),
				condvar:              Condvar::new(),
				update_topology_only: AtomicBool::new(false)
			},
			pool:      Mutex::new(Pool {
				len:         0,
				streams:     Vec::with_capacity(client.options.pool_options.min_pool_size),
				iteration:   0
			}),
			wait_lock: Condvar::new()
		}));

		// start server monitor
		
		let cloned = self_.clone();
		std::thread::Builder::new()
			.name("server-monitor".to_string())
			.spawn(move || cloned.run())?;
		
		// dispatch event
		
		client.dispatch_event(Event::PoolCreated {
			address: (&self_.host_name, self_.port),
			options: ()
		})?;
		
		Ok(self_)
	}
	
	pub fn acquire_stream(&self, client: &Client) -> Result<Connection> {
		let mut pool = self.pool.lock()?;
		client.dispatch_event(Event::ConnectionCheckOutStarted {
			address: (&self.host_name, self.port),
		})?;
		
		loop {
			// fill pool
			while pool.len < client.options.pool_options.min_pool_size {
				client.dispatch_event(Event::ConnectionCreated {
					address:       (&self.host_name, self.port),
					connection_id: pool.len + 1
				})?;
				
				let mut description = self.description.write()?;
				let mut stream = Stream::connect(
					&self.host_name, self.port, &client.options,
					description.compression.first().cloned())?;
				
				// update description
				let ts = SystemTime::now();
				if description.update(stream.handshake(&client.options, true, true), ts)? {
					self.monitor.update_topology_only.store(true, Ordering::Release);
					self.monitor.condvar.notify_all();
				}
				
				pool.len += 1;
				let __tmp__ = pool.len;
				pool.streams.push((stream, __tmp__, SystemTime::now()));
				
				client.dispatch_event(Event::ConnectionReady {
					address:       (&self.host_name, self.port),
					connection_id: pool.len + 1
				})?;
			}
			
			// retrieve open connection from pool
			if let Some((stream, id, ts)) = pool.streams.pop() {
				// close connection if it has been idle for too long
				if client.options.pool_options.max_idle_time_ms != 0
					&& ts.elapsed().unwrap().as_millis() as usize > client.options.pool_options.max_idle_time_ms {
					client.dispatch_event(Event::ConnectionClosed {
						address:       (&self.host_name, self.port),
						connection_id: id,
						reason:        ConnectionClosedReason::Idle
					})?;
					continue;
				}
				
				client.dispatch_event(Event::ConnectionCheckedOut {
					address:       (&self.host_name, self.port),
					connection_id: id
				})?;
				
				return Ok(Connection {
					id,
					inner:     stream,
					server:    self.clone(),
					iteration: pool.iteration,
					has_error: false,
					client:    client.clone()
				})
			}
			
			// continue if pool has reached its max size
			
			if client.options.pool_options.max_pool_size != 0
				&& pool.len == client.options.pool_options.max_pool_size {
				pool = self.wait_lock.wait(pool)?;
				continue;
			}
			
			// open new connection
			
			client.dispatch_event(Event::ConnectionCreated {
				address:       (&self.host_name, self.port),
				connection_id: pool.len + 1
			})?;
			
			let mut description = self.description.write()?;
			let mut stream = Stream::connect(&self.host_name, self.port, &client.options,
					description.compression.first().cloned())?;
			
			// update description
			
			let ts = SystemTime::now();
			match description.update(
				stream.handshake(&client.options, true, true), ts) {
				Ok(false) => (),
				Ok(true) => {
					self.monitor.update_topology_only.store(true, Ordering::Release);
					self.monitor.condvar.notify_all();
				}
				Err(err) => {
					client.dispatch_event(Event::ConnectionCheckOutFailed {
						address: (&self.host_name, self.port),
						reason: ConnectionCheckOutFailedReason::Error
					})?;
					pool.iteration += 1;
					pool.streams.clear();
					pool.len = 0;
					client.dispatch_event(Event::PoolCleared {
						address: (&self.host_name, self.port)
					})?;
					return Err(err);
				}
			}
			
			client.dispatch_event(Event::ConnectionReady {
				address:       (&self.host_name, self.port),
				connection_id: pool.len + 1
			})?;
			
			client.dispatch_event(Event::ConnectionCheckedOut {
				address:       (&self.host_name, self.port),
				connection_id: pool.len + 1
			})?;
			
			pool.len += 1;
			return Ok(Connection {
				id:        pool.len,
				inner:     stream,
				server:    self.clone(),
				iteration: pool.iteration,
				has_error: false,
				client:    client.clone()
			})
		}
	}
	
	/// force server monitor to update
	pub fn update(&self) {
		self.monitor.condvar.notify_one();
	}
	
	/// runs the server monitor, panics only if the dummy lock cannot be acquired
	fn run(self) {
		while let Err(err) = self.run_inner() {
			eprintln!("[MONGODB] server monitor for {}:{} died (`{}`), restarting in {} ms",
					  self.host_name, self.port, err, self.monitor.heartbeat_frequency.as_millis());
			std::mem::drop(self.monitor.condvar.wait_timeout(
				self.monitor.dummy_lock.lock().unwrap(),
				self.monitor.heartbeat_frequency
			).unwrap().0);
		}
	}
	
	fn run_inner(&self) -> Result<()> {
		let mut stream = Stream::connect(
			&self.host_name, self.port, &self.monitor.client.upgrade().unwrap().options, None)?;
		let mut guard = self.monitor.dummy_lock.lock()?;
		while Arc::strong_count(&self.0) > 1 {
			let client = match self.monitor.client.upgrade() {
				None => break, // client has been dropped, kill monitor
				Some(client) => Client(client)
			};
			
			let ts = SystemTime::now();
			if self.monitor.update_topology_only.swap(false, Ordering::AcqRel)
				|| self.description.write()?.update(stream.handshake(&client.options, false, false), ts)? {
				client.topology.write()?.update(self, &client)?;
			}

			let elapsed = ts.elapsed().unwrap();
			if elapsed >= self.monitor.heartbeat_frequency {
				continue;
			}
			
			guard = self.monitor.condvar.wait_timeout(
				guard, self.monitor.heartbeat_frequency - elapsed)?.0;
		}
		Ok(())
	}
}

#[derive(Debug)]
pub struct Connection {
	pub id:    usize,
	inner:     Stream,
	server:    Server,
	iteration: usize,
	has_error: bool,
	client:    Client
}

impl Drop for Connection {
	fn drop(&mut self) {
		if self.has_error {
			self.client.dispatch_event(Event::ConnectionClosed {
				address:       (&self.server.host_name, self.server.port),
				connection_id: self.id,
				reason:        ConnectionClosedReason::Error
			}).unwrap_or(());
			return;
		}
		
		match self.server.pool.lock() {
			Ok(ref mut pool) if pool.iteration == self.iteration => {
				self.client.dispatch_event(Event::ConnectionCheckedIn {
					address:       (&self.server.host_name, self.server.port),
					connection_id: self.id
				}).unwrap_or(());
				pool.streams.push((std::mem::replace(&mut self.inner, Stream::Empty),
								   self.id, SystemTime::now()));
				self.server.wait_lock.notify_one();
			}
			_ => self.client.dispatch_event(Event::ConnectionClosed {
				address:       (&self.server.host_name, self.server.port),
				connection_id: self.id,
				reason:        ConnectionClosedReason::Stale
			}).unwrap_or(())
		}
	}
}

impl std::ops::Deref for Connection {
	type Target = Stream;
	
	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl std::ops::DerefMut for Connection {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.inner
	}
}
