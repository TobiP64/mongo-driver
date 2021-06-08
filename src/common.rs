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
	crate::{
		Client,
		bson::{self, Document, Timestamp},
		wire::{InvalidReplyError, GenericReply, Compressor, SUPPORTED_COMPRESSORS},
		topology::{TopologyType, ServerSelectionError},
		__DebugWrapper__
	},
	std::{str::FromStr, sync::Arc, time::Duration},
	serde::{Serialize, Deserialize}
};

pub const DEFAULT_MONGO_PORT:               u16   = 27017;
pub const DEFAULT_CONNECTION_TIMEOUT_MS:    usize = 10_000;
pub const DEFAULT_MIN_POOL_SIZE:            usize = 0;
pub const DEFAULT_MAX_POOL_SIZE:            usize = 100;
pub const DEFAULT_MAX_IDLE_TIME_MS:         usize = 0;
pub const DEFAULT_LOCAL_THRESHOLD:          Duration = Duration::from_micros(15);
pub const DEFAULT_SERVER_SELECTION_TIMEOUT: Duration = Duration::from_secs(30);
pub const DEFAULT_HEARTBEAT_FREQUENCY:      Duration = Duration::from_secs(10);
pub const DEFAULT_MAX_STALENESS_SECONDS:    isize = -1;

/// see https://github.com/mongodb/specifications/blob/master/source/connection-string/connection-string-spec.rst,
/// https://github.com/mongodb/specifications/blob/master/source/uri-options/uri-options.rst
#[derive(Debug, Clone)]
pub struct ClientOptions {
	pub hosts:                     Vec<String>,
	pub appname:                   Option<String>,
	pub compressors:               Option<Vec<Compressor>>,
	pub connection_timeout_ms:     usize,
	pub replica_set:               Option<String>,
	pub retry_reads:               bool,
	pub retry_writes:              bool,
	pub zlib_compression_level:    Option<isize>,
	pub credential:                Credential,
	pub server_selection_config:   ServerSelectionConfig,
	pub pool_options:              ConnectionPoolOptions,
	pub read_preference:           ReadPreference,
	pub read_concern:              Option<ReadConcern>,
	pub write_concern:             Option<WriteConcern>,
	#[cfg(feature = "tls")]
	pub tls_options:               Option<TlsOptions>,
	#[cfg(feature = "tls")]
	pub tls_config:                Option<__DebugWrapper__<Arc<rustls::ClientConfig>>>,
	pub initial_topology_type:     Option<TopologyType>
}

#[derive(Debug)]
pub enum ClientOptionsParseError {
	InvalidScheme,
	InvalidKey(String),
	InvalidValue { key: &'static str, val: Box<dyn std::fmt::Debug + Send + Sync + 'static> }
}

impl<T: std::fmt::Debug + Send + Sync + 'static> From<(&'static str, T)> for ClientOptionsParseError {
	fn from((key, val): (&'static str, T)) -> Self {
		Self::InvalidValue { key, val: Box::new(val) }
	}
}

impl ClientOptions {
	pub fn connect(self) -> Result<Client> {
		Client::new(self)
	}
}

impl Default for ClientOptions {
	fn default() -> Self {
		Self {
			hosts:                     Vec::new(),
			appname:                   None,
			compressors:               None,
			connection_timeout_ms:     DEFAULT_CONNECTION_TIMEOUT_MS,
			replica_set:               None,
			retry_reads:               true,
			retry_writes:              true,
			zlib_compression_level:    None,
			credential:                Credential::default(),
			server_selection_config:   ServerSelectionConfig::default(),
			pool_options:              ConnectionPoolOptions::default(),
			read_preference:           ReadPreference::default(),
			read_concern:              None,
			write_concern:             None,
			#[cfg(feature = "tls")]
			tls_options:               None,
			#[cfg(feature = "tls")]
			tls_config:                None,
			initial_topology_type:     None
		}
	}
}

impl FromStr for ClientOptions {
	type Err = Error;
	
	#[allow(clippy::unit_arg)]
	fn from_str(mut s: &str) -> std::result::Result<Self, Self::Err> {
		let mut self_ = Self::default();
		
		if !s.starts_with("mongodb://") {
			return Err(Error::InvalidClientOptions(ClientOptionsParseError::InvalidScheme));
		}
		
		s = s.trim_start_matches("mongodb://");
		
		if let Some(i) = s.find('@') {
			let mut split = s[..i].split(':');
			self_.credential.username = split.next().map(str::to_string);
			self_.credential.password = split.next().map(str::to_string);
			s = &s[i + 1..];
		}
		
		let i = s.find('/').unwrap_or_else(|| s.len());
		self_.hosts = s[..i].split(',').map(String::from).collect();
		
		if i == s.len() { return Ok(self_); }
		
		s = &s[i + 1..];
		
		let i = s.find('?').unwrap_or_else(|| s.len());
		self_.credential.source = Some(s[..i].to_string());
		s = &s[i + 1..];
		
		// options
		
		 s.split('&').map(|s| {
			let i = s.find('=').unwrap_or_else(|| s.len());
			(&s[..i], &s[i + 1..])
		}).try_for_each(|(key, value)| Ok::<_, ClientOptionsParseError>(match key {
			 "appname"                      => self_.appname = Some(value.to_string()),
			 "authMechanism"                => self_.credential.mechanism = Some(value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("authMechanism", e)))?),
			 "authMechanismProperties"      => self_.credential.mechanism_properties = Some(value.to_string()),
			 "authSource"                   => self_.credential.source = Some(value.to_string()),
			 "compressors"                  => self_.compressors = Some(value.split(',')
				 .map(std::str::FromStr::from_str)
				 .filter(|s| if let Ok(s) = s { SUPPORTED_COMPRESSORS.contains(s) } else { true })
				 .collect::<std::result::Result<_, _>>()
				 .map_err(|_| ClientOptionsParseError::from(("compressors", value.to_string())))?),
			 "connectTimeoutMS"             => self_.connection_timeout_ms = value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("connectTimeoutMS", e)))?,
			 "heartbeatFrequencyMS"         => self_.server_selection_config.heartbeat_frequency = Duration::from_millis(value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("heartbeatFrequencyMS", e)))?),
			 "journal"                      => self_.write_concern
				 .get_or_insert_with(WriteConcern::default).journal = Some(value == "true"),
			 "localThresholdMS"             => self_.server_selection_config.local_threshold = Duration::from_millis(value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("localThresholdMS", e)))?),
			 "maxIdleTimeMS"                => self_.pool_options.max_idle_time_ms = value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("maxIdleTimeMS", e)))?,
			 "maxPoolSize"                  => self_.pool_options.max_pool_size = value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("maxPoolSize", e)))?,
			 "maxStalenessSeconds"          => self_.read_preference.max_staleness_seconds = value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("maxStalenessSeconds", e)))?,
			 "minPoolSize"                  => self_.pool_options.min_pool_size = value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("minPoolSize", e)))?,
			 "readConcernLevel"             => self_.read_concern
				 .get_or_insert_with(ReadConcern::default).level = Some(value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("readConcernLevel", e)))?),
			 "readPreference"               => self_.read_preference.mode = value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("readPreference", e)))?,
			 "readPreferenceTags"           => self_.read_preference.tag_sets.push(value.split(',').map(|s| {
				 let i = s.find(':').unwrap_or_else(|| s.len());
				 (s[..i].to_string(), s[i + 1..].to_string())
			 }).collect()),
			 "replicaSet"                   => self_.replica_set = Some(value.to_string()),
			 "retryReads"                   => self_.retry_reads = value == "true",
			 "retryWrites"                  => self_.retry_writes = value == "true",
			 "serverSelectionTimeoutMS"     => self_.server_selection_config.server_selection_timeout = Duration::from_millis(value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("serverSelectionTimeoutMS", e)))?),
			 "tls"                          => self_.tls_options = Some(TlsOptions::default()),
			 "tlsAllowInvalidCertificates"  => self_.tls_options
				 .get_or_insert_with(TlsOptions::default).allow_invalid_certificates = value == "true",
			 "tlsAllowInvalidHostnames"     => self_.tls_options
				 .get_or_insert_with(TlsOptions::default).allow_invalid_hostnames = value == "true",
			 "tlsCAFile"                    => self_.tls_options
				 .get_or_insert_with(TlsOptions::default).ca_file = Some(value.to_string()),
			 "tlsCertificateKeyFile"        => self_.tls_options
				 .get_or_insert_with(TlsOptions::default).certificate_key_file = Some(value.to_string()),
			 "tlsCertificateKeyFilePassword"=> self_.tls_options
				 .get_or_insert_with(TlsOptions::default).certificate_key_file_password = Some(value.to_string()),
			 "tlsInsecure"                  => self_.tls_options
				 .get_or_insert_with(TlsOptions::default).insecure = value == "true",
			 "w"                            => self_.write_concern
				 .get_or_insert_with(WriteConcern::default).w = Some(value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("w", e)))?),
			 "wTimeoutMS"                   => self_.write_concern
				 .get_or_insert_with(WriteConcern::default).w_timeout_ms = Some(value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("wTimeoutMS", e)))?),
			 "zlibCompressionLevel"         => self_.zlib_compression_level = Some(value.parse()
				 .map_err(|e| ClientOptionsParseError::from(("zlibCompressionLevel", e)))?),
			 "initialTopologyType"          => self_.initial_topology_type = Some(match value {
				 "unknown"               => TopologyType::Unknown,
				 "sharded"               => TopologyType::Sharded,
				 "replicaSetWithPrimary" => TopologyType::ReplicaSetWithPrimary,
				 "replicaSetNoPrimary"   => TopologyType::ReplicaSetNoPrimary,
				 _ => panic!("invalid topology type: `{}`", value)
			 }),
			 key => return Err(ClientOptionsParseError::InvalidKey(key.to_string()))
		 }))?;
		
		// create tls config
		
		if let Some(options) = &self_.tls_options {
			let mut config = rustls::ClientConfig::new();
			
			if let (Some(key), Some(cert)) = (&options.certificate_key_file, &options.ca_file) {
				config.set_single_client_cert(
					rustls::internal::pemfile::certs(
						&mut std::io::BufReader::new(std::fs::File::open(cert)?)).unwrap(),
					rustls::internal::pemfile::pkcs8_private_keys(
						&mut std::io::BufReader::new(std::fs::File::open(key)?)).unwrap()
						.remove(0)
				);
			}
			
			self_.tls_config = Some(__DebugWrapper__(Arc::new(config)));
		}
		
		Ok(self_)
	}
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct Credential {
	pub username:             Option<String>,
	pub password:             Option<String>,
	pub source:               Option<String>,
	pub mechanism:            Option<AuthMech>,
	pub mechanism_properties: Option<String>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize)]
pub enum AuthMech {
	#[serde(rename = "MONGODB-X509")]
	MongoDbX509,
	#[serde(rename = "GSSAPI")]
	GssApi,
	#[serde(rename = "PLAIN")]
	Plain,
	#[serde(rename = "SCRAM-SHA-1")]
	ScramSha1,
	#[serde(rename = "SCRAM-SHA-256")]
	ScramSha256,
	#[serde(rename = "MONGODB-AWS")]
	MongoDbAws
}

impl FromStr for AuthMech {
	type Err = ();

	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		Ok(match s {
			"MONGODB-X509"  => Self::MongoDbX509,
			"GSSAPI"        => Self::GssApi,
			"PLAIN"         => Self::Plain,
			"SCRAM-SHA-1"   => Self::ScramSha1,
			"SCRAM-SHA-256" => Self::ScramSha256,
			"MONGODB-AWS"   => Self::MongoDbAws,
			_               => return Err(())
		})
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ConnectionPoolOptions {
	pub max_pool_size:    usize,
	pub min_pool_size:    usize,
	pub max_idle_time_ms: usize
}

impl Default for ConnectionPoolOptions {
	fn default() -> Self {
		Self {
			max_pool_size:    DEFAULT_MAX_POOL_SIZE,
			min_pool_size:    DEFAULT_MIN_POOL_SIZE,
			max_idle_time_ms: DEFAULT_MAX_IDLE_TIME_MS
		}
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ServerSelectionConfig {
	pub local_threshold:          Duration,
	pub server_selection_timeout: Duration,
	pub heartbeat_frequency:      Duration
}

impl Default for ServerSelectionConfig {
	fn default() -> Self {
		Self {
			local_threshold:          DEFAULT_LOCAL_THRESHOLD,
			server_selection_timeout: DEFAULT_SERVER_SELECTION_TIMEOUT,
			heartbeat_frequency:      DEFAULT_HEARTBEAT_FREQUENCY
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReadPreference {
	pub mode:                  ReadPreferenceMode,
	pub max_staleness_seconds: isize,
	pub tag_sets:              Vec<std::collections::HashMap<String, String>>,
}

impl Default for ReadPreference {
	fn default() -> Self {
		Self {
			mode:                  ReadPreferenceMode::Primary,
			max_staleness_seconds: DEFAULT_MAX_STALENESS_SECONDS,
			tag_sets:              vec![]
		}
	}
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum ReadPreferenceMode {
	Primary,
	PrimaryPreferred,
	Secondary,
	SecondaryPreferred,
	Nearest
}

impl FromStr for ReadPreferenceMode {
	type Err = ();
	
	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		Ok(match s {
			"primary"            => Self::Primary,
			"primaryPreferred"   => Self::PrimaryPreferred,
			"secondary"          => Self::Secondary,
			"secondaryPreferred" => Self::SecondaryPreferred,
			"nearest"            => Self::Nearest,
			_ => return Err(())
		})
	}
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Serialize)]
pub struct ReadConcern {
	#[serde(skip_serializing_if = "Option::is_none")]
	level: Option<ReadConcernLevel>
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadConcernLevel {
	Local,
	Majority,
	Linearizable,
	Available
}

impl FromStr for ReadConcernLevel {
	type Err = ();
	
	fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
		Ok(match s {
			"local"        => Self::Local,
			"majority"     => Self::Majority,
			"linearizable" => Self::Linearizable,
			"available"    => Self::Available,
			_ => return Err(())
		})
	}
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteConcern {
	#[serde(skip_serializing_if = "Option::is_none")]
	journal:      Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	w:            Option<i32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	w_timeout_ms: Option<i64>
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct TlsOptions {
	pub allow_invalid_certificates:    bool,
	pub allow_invalid_hostnames:       bool,
	pub ca_file:                       Option<String>,
	pub certificate_key_file:          Option<String>,
	pub certificate_key_file_password: Option<String>,
	pub insecure:                      bool,
}

/// see https://docs.mongodb.com/manual/reference/command/aggregate/
#[derive(Debug, Default, Clone, PartialEq)]
pub struct AggregateOptions {
	/// Enables writing to temporary files. When set to true, aggregation stages
	/// can write data to the _tmp subdirectory in the dbPath directory.
	///
	/// This option is sent only if the caller explicitly provides a value. The default
	/// is to not send a value.
	pub allow_disk_usage:           Option<bool>,
	/// The number of documents to return per batch.
	/// If specified, drivers SHOULD apply this option to both the original aggregate command and subsequent
	/// getMore operations on the cursor.
	///
	/// Drivers MUST NOT specify a batchSize of zero in an aggregate command that includes an $out or $merge stage,
	/// as that will prevent the pipeline from executing. Drivers SHOULD leave the cursor.batchSize command option
	/// unset in an aggregate command that includes an $out or $merge stage.
	pub batch_size:                 Option<i64>,
	/// If true, allows the write to opt-out of document level validation. This only applies
	/// when the $out or $merge stage is specified.
	///
	/// This option is sent only if the caller explicitly provides a true value. The default is to not send a value.
	/// For servers < 3.2, this option is ignored and not sent as document validation is not available.
	pub bypass_document_validation: Option<bool>,
	/// Specifies a collation.
	/// For servers < 3.4, the driver MUST raise an error if the caller explicitly provides a value.
	pub collation:                  Option<Collation>,
	/// The maximum amount of time to allow the query to run.
	pub max_time_ms:                Option<i64>,
	/// The maximum amount of time for the server to wait on new documents to satisfy a tailable cursor
	/// query.
	///
	/// This options only applies to aggregations which return a TAILABLE_AWAIT cursor. Drivers
	/// SHOULD always send this value, if the cursor is not a TAILABLE_AWAIT cursor the server will
	/// ignore it.
	///
	/// note: this option is an alias for `maxTimeMS`, used on `getMore` commands
	/// note: this option is not set on the `aggregate` command
	pub max_await_time_ms:          Option<i64>,
	/// Enables users to specify an arbitrary string to help trace the operation through
	/// the database profiler, currentOp and logs. The default is to not send a value.
	pub comment:                    Option<String>,
	/// The index to use for the aggregation. The hint does not apply to $lookup and $graphLookup stages.
	pub hint:                       Option<Document>
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct Collation {
	pub locale:           String,
	pub case_level:       Option<bool>,
	pub case_first:       Option<String>,
	pub strength:         Option<isize>,
	pub numeric_ordering: Option<bool>,
	pub alternate:        Option<String>,
	pub max_variable:     Option<String>,
	pub normalization:    Option<bool>,
	pub backwards:        Option<bool>
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ChangeStreamOptions<'a> {
	pub full_document:           Option<&'a str>,
	pub resume_after:            Option<Document>,
	pub max_await_time_ms:       Option<i64>,
	pub batch_size:              Option<i64>,
	pub collation:               Option<Collation>,
	pub start_at_operation_time: Option<Timestamp>,
	pub start_after:             Option<Document>,
	pub all_changes_for_cluster: Option<bool>
}

#[derive(Serialize)]
pub(super) struct ChangeStreamStage<'a> {
	#[serde(rename = "$changeStream")]
	pub(super) change_stream: ChangeStreamOptions<'a>
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeStreamDocument {
	pub _id:                     Document,
	pub operation_type:          String,
	pub full_document:           Option<Document>,
	pub ns:                      Option<ChangeStreamNs>,
	pub to:                      Option<ChangeStreamNs>,
	pub document_key:            Option<Document>,
	pub update_description:      Option<UpdateDescription>,
	pub cluster_time:            Option<Timestamp>,
	pub txn_number:              Option<i64>,
	pub lsid:                    Option<ChangeStreamLsId>
}

#[derive(Deserialize)]
pub struct ChangeStreamNs {
	pub db:   String,
	pub coll: Option<String>
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDescription {
	pub updated_fields: Document,
	pub removed_fields: Vec<String>
}

#[derive(Deserialize)]
pub struct ChangeStreamLsId {
	#[serde(with = "serde_bytes")]
	pub id: Vec<u8>,
	#[serde(with = "serde_bytes")]
	pub uid: Vec<u8>
}

pub type MDBResult<T> = std::result::Result<T, Error>;
pub(super) type Result<T> = MDBResult<T>;

#[derive(Debug)]
pub enum Error {
	InvalidClientOptions(ClientOptionsParseError),
	Sync,
	Io(std::io::Error),
	Tls(rustls::TLSError),
	Dns(webpki::InvalidDNSNameError),
	ServerSelection(ServerSelectionError),
	InvalidReply(InvalidReplyError),
	Operation(MongoError, String),
	BsonEncode(bson::se::Error),
	BsonDecode(bson::de::Error),
	#[cfg(feature = "auth")]
	Auth(crate::auth::AuthError),
	#[cfg(not(feature = "auth"))]
	AuthUnsupported
}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		<Self as std::fmt::Debug>::fmt(self, f)
	}
}

impl From<ClientOptionsParseError> for Error {
	fn from(e: ClientOptionsParseError) -> Self {
		Self::InvalidClientOptions(e)
	}
}

impl From<std::io::Error> for Error {
	fn from(e: std::io::Error) -> Self {
		Self::Io(e)
	}
}

impl<T> From<std::sync::PoisonError<T>> for Error {
	fn from(_: std::sync::PoisonError<T>) -> Self {
		Self::Sync
	}
}

impl From<rustls::TLSError> for Error {
	fn from(e: rustls::TLSError) -> Self {
		Self::Tls(e)
	}
}

impl From<webpki::InvalidDNSNameError> for Error {
	fn from(e: webpki::InvalidDNSNameError) -> Self {
		Self::Dns(e)
	}
}

impl From<GenericReply> for Error {
	fn from(reply: GenericReply) -> Self {
		Self::Operation((reply.code.unwrap() as i16).into(), reply.errmsg.unwrap())
	}
}

impl From<(MongoError, String)> for Error {
	fn from((e, s): (MongoError, String)) -> Self {
		Self::Operation(e, s)
	}
}

impl From<bson::se::Error> for Error {
	fn from(v: bson::se::Error) -> Self {
		match v {
			bson::se::Error::Io(err) => Self::Io(err),
			v => Self::BsonEncode(v)
		}
	}
}

impl From<bson::de::Error> for Error {
	fn from(v: bson::de::Error) -> Self {
		match v {
			bson::de::Error::Io(err) => Self::Io(err),
			v => Self::BsonDecode(v)
		}
	}
}

#[repr(u16)]
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum MongoError {
	Ok                                               = 0,
	InternalError                                    = 1,
	BadValue                                         = 2,
	NoSuchKey                                        = 4,
	GraphContainsCycle                               = 5,
	HostUnreachable                                  = 6,
	HostNotFound                                     = 7,
	UnknownError                                     = 8,
	FailedToParse                                    = 9,
	CannotMutateObject                               = 10,
	UserNotFound                                     = 11,
	UnsupportedFormat                                = 12,
	Unauthorized                                     = 13,
	TypeMismatch                                     = 14,
	Overflow                                         = 15,
	InvalidLength                                    = 16,
	ProtocolError                                    = 17,
	AuthenticationFailed                             = 18,
	CannotReuseObject                                = 19,
	IllegalOperation                                 = 20,
	EmptyArrayOperation                              = 21,
	InvalidBSON                                      = 22,
	AlreadyInitialized                               = 23,
	LockTimeout                                      = 24,
	RemoteValidationError                            = 25,
	NamespaceNotFound                                = 26,
	IndexNotFound                                    = 27,
	PathNotViable                                    = 28,
	NonExistentPath                                  = 29,
	InvalidPath                                      = 30,
	RoleNotFound                                     = 31,
	RolesNotRelated                                  = 32,
	PrivilegeNotFound                                = 33,
	CannotBackfillArray                              = 34,
	UserModificationFailed                           = 35,
	RemoteChangeDetected                             = 36,
	FileRenameFailed                                 = 37,
	FileNotOpen                                      = 38,
	FileStreamFailed                                 = 39,
	ConflictingUpdateOperators                       = 40,
	FileAlreadyOpen                                  = 41,
	LogWriteFailed                                   = 42,
	CursorNotFound                                   = 43,
	UserDataInconsistent                             = 45,
	LockBusy                                         = 46,
	NoMatchingDocument                               = 47,
	NamespaceExists                                  = 48,
	InvalidRoleModification                          = 49,
	MaxTimeMSExpired                                 = 50,
	ManualInterventionRequired                       = 51,
	DollarPrefixedFieldName                          = 52,
	InvalidIdField                                   = 53,
	NotSingleValueField                              = 54,
	InvalidDBRef                                     = 55,
	EmptyFieldName                                   = 56,
	DottedFieldName                                  = 57,
	RoleModificationFailed                           = 58,
	CommandNotFound                                  = 59,
	ShardKeyNotFound                                 = 61,
	OplogOperationUnsupported                        = 62,
	StaleShardVersion                                = 63,
	WriteConcernFailed                               = 64,
	MultipleErrorsOccurred                           = 65,
	ImmutableField                                   = 66,
	CannotCreateIndex                                = 67 ,
	IndexAlreadyExists                               = 68 ,
	AuthSchemaIncompatible                           = 69,
	ShardNotFound                                    = 70,
	ReplicaSetNotFound                               = 71,
	InvalidOptions                                   = 72,
	InvalidNamespace                                 = 73,
	NodeNotFound                                     = 74,
	WriteConcernLegacyOK                             = 75,
	NoReplicationEnabled                             = 76,
	OperationIncomplete                              = 77,
	CommandResultSchemaViolation                     = 78,
	UnknownReplWriteConcern                          = 79,
	RoleDataInconsistent                             = 80,
	NoMatchParseContext                              = 81,
	NoProgressMade                                   = 82,
	RemoteResultsUnavailable                         = 83,
	DuplicateKeyValue                                = 84,
	IndexOptionsConflict                             = 85,
	IndexKeySpecsConflict                            = 86,
	CannotSplit                                      = 87,
	NetworkTimeout                                   = 89,
	CallbackCanceled                                 = 90,
	ShutdownInProgress                               = 91,
	SecondaryAheadOfPrimary                          = 92,
	InvalidReplicaSetConfig                          = 93,
	NotYetInitialized                                = 94,
	NotSecondary                                     = 95,
	OperationFailed                                  = 96,
	NoProjectionFound                                = 97,
	DBPathInUse                                      = 98,
	UnsatisfiableWriteConcern                        = 100,
	OutdatedClient                                   = 101,
	IncompatibleAuditMetadata                        = 102,
	NewReplicaSetConfigurationIncompatible           = 103,
	NodeNotElectable                                 = 104,
	IncompatibleShardingMetadata                     = 105,
	DistributedClockSkewed                           = 106,
	LockFailed                                       = 107,
	InconsistentReplicaSetNames                      = 108,
	ConfigurationInProgress                          = 109,
	CannotInitializeNodeWithData                     = 110,
	NotExactValueField                               = 111,
	WriteConflict                                    = 112,
	InitialSyncFailure                               = 113,
	InitialSyncOplogSourceMissing                    = 114,
	CommandNotSupported                              = 115,
	DocTooLargeForCapped                             = 116,
	ConflictingOperationInProgress                   = 117,
	NamespaceNotSharded                              = 118,
	InvalidSyncSource                                = 119,
	OplogStartMissing                                = 120,
	DocumentValidationFailure                        = 121,
	NotAReplicaSet                                   = 123,
	IncompatibleElectionProtocol                     = 124,
	CommandFailed                                    = 125,
	RPCProtocolNegotiationFailed                     = 126,
	UnrecoverableRollbackError                       = 127,
	LockNotFound                                     = 128,
	LockStateChangeFailed                            = 129,
	SymbolNotFound                                   = 130,
	FailedToSatisfyReadPreference                    = 133,
	ReadConcernMajorityNotAvailableYet               = 134,
	StaleTerm                                        = 135,
	CappedPositionLost                               = 136,
	IncompatibleShardingConfigVersion                = 137,
	RemoteOplogStale                                 = 138,
	JSInterpreterFailure                             = 139,
	InvalidSSLConfiguration                          = 140,
	SSLHandshakeFailed                               = 141,
	JSUncatchableError                               = 142,
	CursorInUse                                      = 143,
	IncompatibleCatalogManager                       = 144,
	PooledConnectionsDropped                         = 145,
	ExceededMemoryLimit                              = 146,
	ZLibError                                        = 147,
	ReadConcernMajorityNotEnabled                    = 148,
	NoConfigMaster                                   = 149,
	StaleEpoch                                       = 150,
	OperationCannotBeBatched                         = 151,
	OplogOutOfOrder                                  = 152,
	ChunkTooBig                                      = 153,
	InconsistentShardIdentity                        = 154,
	CannotApplyOplogWhilePrimary                     = 155,
	CanRepairToDowngrade                             = 157,
	MustUpgrade                                      = 158,
	DurationOverflow                                 = 159,
	MaxStalenessOutOfRange                           = 160,
	IncompatibleCollationVersion                     = 161,
	CollectionIsEmpty                                = 162,
	ZoneStillInUse                                   = 163,
	InitialSyncActive                                = 164,
	ViewDepthLimitExceeded                           = 165,
	CommandNotSupportedOnView                        = 166,
	OptionNotSupportedOnView                         = 167,
	InvalidPipelineOperator                          = 168,
	CommandOnShardedViewNotSupportedOnMongod         = 169,
	TooManyMatchingDocuments                         = 170,
	CannotIndexParallelArrays                        = 171,
	TransportSessionClosed                           = 172,
	TransportSessionNotFound                         = 173,
	TransportSessionUnknown                          = 174,
	QueryPlanKilled                                  = 175,
	FileOpenFailed                                   = 176,
	ZoneNotFound                                     = 177,
	RangeOverlapConflict                             = 178,
	WindowsPdhError                                  = 179,
	BadPerfCounterPath                               = 180,
	AmbiguousIndexKeyPattern                         = 181,
	InvalidViewDefinition                            = 182,
	ClientMetadataMissingField                       = 183,
	ClientMetadataAppNameTooLarge                    = 184,
	ClientMetadataDocumentTooLarge                   = 185,
	ClientMetadataCannotBeMutated                    = 186,
	LinearizableReadConcernError                     = 187,
	IncompatibleServerVersion                        = 188,
	PrimarySteppedDown                               = 189,
	MasterSlaveConnectionFailure                     = 190,
	FailPointEnabled                                 = 192,
	NoShardingEnabled                                = 193,
	BalancerInterrupted                              = 194,
	ViewPipelineMaxSizeExceeded                      = 195,
	InvalidIndexSpecificationOption                  = 197,
	ReplicaSetMonitorRemoved                         = 199,
	ChunkRangeCleanupPending                         = 200,
	CannotBuildIndexKeys                             = 201,
	NetworkInterfaceExceededTimeLimit                = 202,
	ShardingStateNotInitialized                      = 203,
	TimeProofMismatch                                = 204,
	ClusterTimeFailsRateLimiter                      = 205,
	NoSuchSession                                    = 206,
	InvalidUUID                                      = 207,
	TooManyLocks                                     = 208,
	StaleClusterTime                                 = 209,
	CannotVerifyAndSignLogicalTime                   = 210,
	KeyNotFound                                      = 211,
	IncompatibleRollbackAlgorithm                    = 212,
	DuplicateSession                                 = 213,
	AuthenticationRestrictionUnmet                   = 214,
	DatabaseDropPending                              = 215,
	ElectionInProgress                               = 216,
	IncompleteTransactionHistory                     = 217,
	UpdateOperationFailed                            = 218,
	FTDCPathNotSet                                   = 219,
	FTDCPathAlreadySet                               = 220,
	IndexModified                                    = 221,
	CloseChangeStream                                = 222,
	IllegalOpMsgFlag                                 = 223,
	QueryFeatureNotAllowed                           = 224,
	TransactionTooOld                                = 225,
	AtomicityFailure                                 = 226,
	CannotImplicitlyCreateCollection                 = 227,
	SessionTransferIncomplete                        = 228,
	MustDowngrade                                    = 229,
	DNSHostNotFound                                  = 230,
	DNSProtocolError                                 = 231,
	MaxSubPipelineDepthExceeded                      = 232,
	TooManyDocumentSequences                         = 233,
	RetryChangeStream                                = 234,
	InternalErrorNotSupported                        = 235,
	ForTestingErrorExtraInfo                         = 236,
	CursorKilled                                     = 237,
	NotImplemented                                   = 238,
	SnapshotTooOld                                   = 239,
	DNSRecordTypeMismatch                            = 240,
	ConversionFailure                                = 241,
	CannotCreateCollection                           = 242,
	IncompatibleWithUpgradedServer                   = 243,
	TransactionAborted                               = 244,
	BrokenPromise                                    = 245,
	SnapshotUnavailable                              = 246,
	ProducerConsumerQueueBatchTooLarge               = 247,
	ProducerConsumerQueueEndClosed                   = 248,
	StaleDbVersion                                   = 249,
	StaleChunkHistory                                = 250,
	NoSuchTransaction                                = 251,
	ReentrancyNotAllowed                             = 252,
	FreeMonHttpInFlight                              = 253,
	FreeMonHttpTemporaryFailure                      = 254,
	FreeMonHttpPermanentFailure                      = 255,
	TransactionCommitted                             = 256,
	TransactionTooLarge                              = 257,
	UnknownFeatureCompatibilityVersion               = 258,
	KeyedExecutorRetry                               = 259,
	InvalidResumeToken                               = 260,
	TooManyLogicalSessions                           = 261,
	ExceededTimeLimit                                = 262,
	OperationNotSupportedInTransaction               = 263,
	TooManyFilesOpen                                 = 264,
	OrphanedRangeCleanUpFailed                       = 265,
	FailPointSetFailed                               = 266,
	PreparedTransactionInProgress                    = 267,
	CannotBackup                                     = 268,
	DataModifiedByRepair                             = 269,
	RepairedReplicaSetNode                           = 270,
	JSInterpreterFailureWithStack                    = 271,
	MigrationConflict                                = 272,
	ProducerConsumerQueueProducerQueueDepthExceeded  = 273,
	ProducerConsumerQueueConsumed                    = 274,
	ExchangePassthrough                              = 275,
	IndexBuildAborted                                = 276,
	AlarmAlreadyFulfilled                            = 277,
	UnsatisfiableCommitQuorum                        = 278,
	ClientDisconnect                                 = 279,
	ChangeStreamFatalError                           = 280,
	WouldChangeOwningShard                           = 283,
	ForTestingErrorExtraInfoWithExtraInfoInNamespace = 284,
	IndexBuildAlreadyInProgress                      = 285,
	ChangeStreamHistoryLost                          = 286,
	ChecksumMismatch                                 = 288,
	Location40414                                    = 40414,
	Location40415                                    = 40415,
	Unknown                                          = 0xFFFF
}

impl From<i32> for MongoError {
	fn from(_: i32) -> Self {
		unimplemented!()
	}
}

impl From<i16> for MongoError {
	fn from(v: i16) -> Self {
		match v as u16 {
			0     => Self::Ok,
			1     => Self::InternalError,
			2     => Self::BadValue,
			4     => Self::NoSuchKey,
			5     => Self::GraphContainsCycle,
			6     => Self::HostUnreachable,
			7     => Self::HostNotFound,
			8     => Self::UnknownError,
			9     => Self::FailedToParse,
			10    => Self::CannotMutateObject,
			11    => Self::UserNotFound,
			12    => Self::UnsupportedFormat,
			13    => Self::Unauthorized,
			14    => Self::TypeMismatch,
			15    => Self::Overflow,
			16    => Self::InvalidLength,
			17    => Self::ProtocolError,
			18    => Self::AuthenticationFailed,
			19    => Self::CannotReuseObject,
			20    => Self::IllegalOperation,
			21    => Self::EmptyArrayOperation,
			22    => Self::InvalidBSON,
			23    => Self::AlreadyInitialized,
			24    => Self::LockTimeout,
			25    => Self::RemoteValidationError,
			26    => Self::NamespaceNotFound,
			27    => Self::IndexNotFound,
			28    => Self::PathNotViable,
			29    => Self::NonExistentPath,
			30    => Self::InvalidPath,
			31    => Self::RoleNotFound,
			32    => Self::RolesNotRelated,
			33    => Self::PrivilegeNotFound,
			34    => Self::CannotBackfillArray,
			35    => Self::UserModificationFailed,
			36    => Self::RemoteChangeDetected,
			37    => Self::FileRenameFailed,
			38    => Self::FileNotOpen,
			39    => Self::FileStreamFailed,
			40    => Self::ConflictingUpdateOperators,
			41    => Self::FileAlreadyOpen,
			42    => Self::LogWriteFailed,
			43    => Self::CursorNotFound,
			45    => Self::UserDataInconsistent,
			46    => Self::LockBusy,
			47    => Self::NoMatchingDocument,
			48    => Self::NamespaceExists,
			49    => Self::InvalidRoleModification,
			50    => Self::MaxTimeMSExpired,
			51    => Self::ManualInterventionRequired,
			52    => Self::DollarPrefixedFieldName,
			53    => Self::InvalidIdField,
			54    => Self::NotSingleValueField,
			55    => Self::InvalidDBRef,
			56    => Self::EmptyFieldName,
			57    => Self::DottedFieldName,
			58    => Self::RoleModificationFailed,
			59    => Self::CommandNotFound,
			61    => Self::ShardKeyNotFound,
			62    => Self::OplogOperationUnsupported,
			63    => Self::StaleShardVersion,
			64    => Self::WriteConcernFailed,
			65    => Self::MultipleErrorsOccurred,
			66    => Self::ImmutableField,
			67    => Self::CannotCreateIndex,
			68    => Self::IndexAlreadyExists,
			69    => Self::AuthSchemaIncompatible,
			70    => Self::ShardNotFound,
			71    => Self::ReplicaSetNotFound,
			72    => Self::InvalidOptions,
			73    => Self::InvalidNamespace,
			74    => Self::NodeNotFound,
			75    => Self::WriteConcernLegacyOK,
			76    => Self::NoReplicationEnabled,
			77    => Self::OperationIncomplete,
			78    => Self::CommandResultSchemaViolation,
			79    => Self::UnknownReplWriteConcern,
			80    => Self::RoleDataInconsistent,
			81    => Self::NoMatchParseContext,
			82    => Self::NoProgressMade,
			83    => Self::RemoteResultsUnavailable,
			84    => Self::DuplicateKeyValue,
			85    => Self::IndexOptionsConflict,
			86    => Self::IndexKeySpecsConflict,
			87    => Self::CannotSplit,
			89    => Self::NetworkTimeout,
			90    => Self::CallbackCanceled,
			91    => Self::ShutdownInProgress,
			92    => Self::SecondaryAheadOfPrimary,
			93    => Self::InvalidReplicaSetConfig,
			94    => Self::NotYetInitialized,
			95    => Self::NotSecondary,
			96    => Self::OperationFailed,
			97    => Self::NoProjectionFound,
			98    => Self::DBPathInUse,
			100   => Self::UnsatisfiableWriteConcern,
			101   => Self::OutdatedClient,
			102   => Self::IncompatibleAuditMetadata,
			103   => Self::NewReplicaSetConfigurationIncompatible,
			104   => Self::NodeNotElectable,
			105   => Self::IncompatibleShardingMetadata,
			106   => Self::DistributedClockSkewed,
			107   => Self::LockFailed,
			108   => Self::InconsistentReplicaSetNames,
			109   => Self::ConfigurationInProgress,
			110   => Self::CannotInitializeNodeWithData,
			111   => Self::NotExactValueField,
			112   => Self::WriteConflict,
			113   => Self::InitialSyncFailure,
			114   => Self::InitialSyncOplogSourceMissing,
			115   => Self::CommandNotSupported,
			116   => Self::DocTooLargeForCapped,
			117   => Self::ConflictingOperationInProgress,
			118   => Self::NamespaceNotSharded,
			119   => Self::InvalidSyncSource,
			120   => Self::OplogStartMissing,
			121   => Self::DocumentValidationFailure,
			123   => Self::NotAReplicaSet,
			124   => Self::IncompatibleElectionProtocol,
			125   => Self::CommandFailed,
			126   => Self::RPCProtocolNegotiationFailed,
			127   => Self::UnrecoverableRollbackError,
			128   => Self::LockNotFound,
			129   => Self::LockStateChangeFailed,
			130   => Self::SymbolNotFound,
			133   => Self::FailedToSatisfyReadPreference,
			134   => Self::ReadConcernMajorityNotAvailableYet,
			135   => Self::StaleTerm,
			136   => Self::CappedPositionLost,
			137   => Self::IncompatibleShardingConfigVersion,
			138   => Self::RemoteOplogStale,
			139   => Self::JSInterpreterFailure,
			140   => Self::InvalidSSLConfiguration,
			141   => Self::SSLHandshakeFailed,
			142   => Self::JSUncatchableError,
			143   => Self::CursorInUse,
			144   => Self::IncompatibleCatalogManager,
			145   => Self::PooledConnectionsDropped,
			146   => Self::ExceededMemoryLimit,
			147   => Self::ZLibError,
			148   => Self::ReadConcernMajorityNotEnabled,
			149   => Self::NoConfigMaster,
			150   => Self::StaleEpoch,
			151   => Self::OperationCannotBeBatched,
			152   => Self::OplogOutOfOrder,
			153   => Self::ChunkTooBig,
			154   => Self::InconsistentShardIdentity,
			155   => Self::CannotApplyOplogWhilePrimary,
			157   => Self::CanRepairToDowngrade,
			158   => Self::MustUpgrade,
			159   => Self::DurationOverflow,
			160   => Self::MaxStalenessOutOfRange,
			161   => Self::IncompatibleCollationVersion,
			162   => Self::CollectionIsEmpty,
			163   => Self::ZoneStillInUse,
			164   => Self::InitialSyncActive,
			165   => Self::ViewDepthLimitExceeded,
			166   => Self::CommandNotSupportedOnView,
			167   => Self::OptionNotSupportedOnView,
			168   => Self::InvalidPipelineOperator,
			169   => Self::CommandOnShardedViewNotSupportedOnMongod,
			170   => Self::TooManyMatchingDocuments,
			171   => Self::CannotIndexParallelArrays,
			172   => Self::TransportSessionClosed,
			173   => Self::TransportSessionNotFound,
			174   => Self::TransportSessionUnknown,
			175   => Self::QueryPlanKilled,
			176   => Self::FileOpenFailed,
			177   => Self::ZoneNotFound,
			178   => Self::RangeOverlapConflict,
			179   => Self::WindowsPdhError,
			180   => Self::BadPerfCounterPath,
			181   => Self::AmbiguousIndexKeyPattern,
			182   => Self::InvalidViewDefinition,
			183   => Self::ClientMetadataMissingField,
			184   => Self::ClientMetadataAppNameTooLarge,
			185   => Self::ClientMetadataDocumentTooLarge,
			186   => Self::ClientMetadataCannotBeMutated,
			187   => Self::LinearizableReadConcernError,
			188   => Self::IncompatibleServerVersion,
			189   => Self::PrimarySteppedDown,
			190   => Self::MasterSlaveConnectionFailure,
			192   => Self::FailPointEnabled,
			193   => Self::NoShardingEnabled,
			194   => Self::BalancerInterrupted,
			195   => Self::ViewPipelineMaxSizeExceeded,
			197   => Self::InvalidIndexSpecificationOption,
			199   => Self::ReplicaSetMonitorRemoved,
			200   => Self::ChunkRangeCleanupPending,
			201   => Self::CannotBuildIndexKeys,
			202   => Self::NetworkInterfaceExceededTimeLimit,
			203   => Self::ShardingStateNotInitialized,
			204   => Self::TimeProofMismatch,
			205   => Self::ClusterTimeFailsRateLimiter,
			206   => Self::NoSuchSession,
			207   => Self::InvalidUUID,
			208   => Self::TooManyLocks,
			209   => Self::StaleClusterTime,
			210   => Self::CannotVerifyAndSignLogicalTime,
			211   => Self::KeyNotFound,
			212   => Self::IncompatibleRollbackAlgorithm,
			213   => Self::DuplicateSession,
			214   => Self::AuthenticationRestrictionUnmet,
			215   => Self::DatabaseDropPending,
			216   => Self::ElectionInProgress,
			217   => Self::IncompleteTransactionHistory,
			218   => Self::UpdateOperationFailed,
			219   => Self::FTDCPathNotSet,
			220   => Self::FTDCPathAlreadySet,
			221   => Self::IndexModified,
			222   => Self::CloseChangeStream,
			223   => Self::IllegalOpMsgFlag,
			224   => Self::QueryFeatureNotAllowed,
			225   => Self::TransactionTooOld,
			226   => Self::AtomicityFailure,
			227   => Self::CannotImplicitlyCreateCollection,
			228   => Self::SessionTransferIncomplete,
			229   => Self::MustDowngrade,
			230   => Self::DNSHostNotFound,
			231   => Self::DNSProtocolError,
			232   => Self::MaxSubPipelineDepthExceeded,
			233   => Self::TooManyDocumentSequences,
			234   => Self::RetryChangeStream,
			235   => Self::InternalErrorNotSupported,
			236   => Self::ForTestingErrorExtraInfo,
			237   => Self::CursorKilled,
			238   => Self::NotImplemented,
			239   => Self::SnapshotTooOld,
			240   => Self::DNSRecordTypeMismatch,
			241   => Self::ConversionFailure,
			242   => Self::CannotCreateCollection,
			243   => Self::IncompatibleWithUpgradedServer,
			244   => Self::TransactionAborted,
			245   => Self::BrokenPromise,
			246   => Self::SnapshotUnavailable,
			247   => Self::ProducerConsumerQueueBatchTooLarge,
			248   => Self::ProducerConsumerQueueEndClosed,
			249   => Self::StaleDbVersion,
			250   => Self::StaleChunkHistory,
			251   => Self::NoSuchTransaction,
			252   => Self::ReentrancyNotAllowed,
			253   => Self::FreeMonHttpInFlight,
			254   => Self::FreeMonHttpTemporaryFailure,
			255   => Self::FreeMonHttpPermanentFailure,
			256   => Self::TransactionCommitted,
			257   => Self::TransactionTooLarge,
			258   => Self::UnknownFeatureCompatibilityVersion,
			259   => Self::KeyedExecutorRetry,
			260   => Self::InvalidResumeToken,
			261   => Self::TooManyLogicalSessions,
			262   => Self::ExceededTimeLimit,
			263   => Self::OperationNotSupportedInTransaction,
			264   => Self::TooManyFilesOpen,
			265   => Self::OrphanedRangeCleanUpFailed,
			266   => Self::FailPointSetFailed,
			267   => Self::PreparedTransactionInProgress,
			268   => Self::CannotBackup,
			269   => Self::DataModifiedByRepair,
			270   => Self::RepairedReplicaSetNode,
			271   => Self::JSInterpreterFailureWithStack,
			272   => Self::MigrationConflict,
			273   => Self::ProducerConsumerQueueProducerQueueDepthExceeded,
			274   => Self::ProducerConsumerQueueConsumed,
			275   => Self::ExchangePassthrough,
			276   => Self::IndexBuildAborted,
			277   => Self::AlarmAlreadyFulfilled,
			278   => Self::UnsatisfiableCommitQuorum,
			279   => Self::ClientDisconnect,
			280   => Self::ChangeStreamFatalError,
			283   => Self::WouldChangeOwningShard,
			284   => Self::ForTestingErrorExtraInfoWithExtraInfoInNamespace,
			285   => Self::IndexBuildAlreadyInProgress,
			286   => Self::ChangeStreamHistoryLost,
			288   => Self::ChecksumMismatch,
			40414 => Self::Location40414,
			40415 => Self::Location40415,
			_     => Self::Unknown
		}
	}
}

impl Default for MongoError {
	fn default() -> Self {
		MongoError::Ok
	}
}

pub(crate) fn bytes_to_hex(src: &[u8], dst: &mut [u8]) {
	fn fmt_digit(v: u8) -> u8 {
		match v {
			0x0 => b'0', 0x1 => b'1', 0x2 => b'2', 0x3 => b'3',
			0x4 => b'4', 0x5 => b'5', 0x6 => b'6', 0x7 => b'7',
			0x8 => b'8', 0x9 => b'9', 0xA => b'A', 0xB => b'B',
			0xC => b'C', 0xD => b'D', 0xE => b'E', 0xF => b'F',
			_ => unreachable!()
		}
	}

	for (i, v) in src.iter().copied().enumerate() {
		dst[i << 1] = fmt_digit(v >> 4);
		dst[(i << 1) + 1] = fmt_digit(v & 0x0F);
	}
}