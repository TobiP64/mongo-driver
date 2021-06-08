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
	crate::{*, common::ChangeStreamStage},
	std::sync::Arc,
	serde::Serialize
};

#[derive(Debug, Clone)]
pub struct DataBase(pub(crate) Arc<DataBaseInner>);

#[derive(Debug, Clone)]
pub struct DataBaseInner {
	pub client:          Client,
	pub name:            String,
	pub read_preference: ReadPreference,
	pub read_concern:    Option<ReadConcern>,
	pub write_concern:   Option<WriteConcern>
}

impl DataBase {
	pub fn collection(&self, name: &str) -> Collection {
		self.collection_with(name, None, None, None)
	}
	
	pub fn collection_with(
		&self,
		name:            &str,
		read_preference: Option<ReadPreference>,
		read_concern:    Option<ReadConcern>,
		write_concern:   Option<WriteConcern>
	) -> Collection {
		Collection {
			db:              DataBase(self.0.clone()),
			name:            name.to_string(),
			read_preference: read_preference.unwrap_or_else(|| self.0.read_preference.clone()),
			read_concern:    read_concern.or(self.0.read_concern),
			write_concern:   write_concern.or(self.0.write_concern)
		}
	}
	
	pub fn run_command<R: DeserializeOwned>(
		&self,
		command:   &impl Serialize,
		read:      Option<&ReadPreference>,
		retryable: bool
	) -> Result<R> {
		#[derive(Serialize)]
		struct DbCommand<'a, S: Serialize> {
			#[serde(flatten)]
			command: &'a S,
			#[serde(rename = "$db")]
			db:      &'a str
		}
		self.0.client.run_command(&DbCommand { db: &self.0.name, command }, read, retryable)
	}
	
	pub fn run_cursor_command(
		&self,
		command: &impl Serialize,
		read:    Option<&ReadPreference>,
		options: CursorOptions,
	) -> Result<Cursor> {
		self.run_command(command, read, true)
			.map(|result: GetMoreCommandResult| Cursor {
				db:              Self(self.0.clone()),
				namespace:       result.cursor.ns,
				batch_size:      options.batch_size,
				max_time_ms:     options.max_time_ms,
				cursor_id:       result.cursor.id,
				buffer:          result.cursor.batch.into(),
				read_preference: options.read_preference
					.unwrap_or_else(|| self.0.read_preference.clone()),
				post_batch_resume_token: result.cursor.post_batch_resume_token
			})
	}
	
	/// Runs an aggregation framework pipeline on the database for pipeline stages
	/// that do not require an underlying collection, such as $currentOp and $listLocalSessions.
	///
	/// Note: result iteration should be backed by a cursor. Depending on the implementation,
	/// the cursor may back the returned Iterable instance or an iterator that it produces.
	///
	/// see https://docs.mongodb.com/manual/reference/command/aggregate/#dbcmd.aggregate
	pub fn aggregate(
		&self,
		pipeline: &[bson::Document],
		options:  Option<AggregateOptions>
	) -> Result<Cursor> {
		let options = options.unwrap_or_default();
		self.run_cursor_command(&AggregateCommand {
			aggregate:                  1i32,
			pipeline,
			explain:                    None,
			allow_disk_use:             options.allow_disk_usage,
			cursor:                     Some(AggregateCommandCursor { batch_size: options.batch_size }),
			max_time_ms:                options.max_time_ms,
			bypass_document_validation: options.bypass_document_validation,
			read_concern:               self.0.read_concern,
			collation:                  options.collation,
			hint:                       options.hint,
			comment:                    options.comment.as_deref(),
			write_concern:              self.0.write_concern
		},
		None,
		CursorOptions {
			batch_size:      options.batch_size,
			max_time_ms:     options.max_time_ms,
			read_preference: Some(self.0.read_preference.clone())
		})
	}
	
	/// Allows a client to observe all changes in a database.
	/// Excludes system collections.
	pub fn watch(
		&self,
		pipeline: &[bson::Document],
		options:  Option<ChangeStreamOptions>
	) -> Result<ChangeStream> {
		self.aggregate(
			{
				let mut pipeline = pipeline.to_vec();
				pipeline.insert(0, Document::from(&ChangeStreamStage {
					change_stream: options.as_ref().map(Clone::clone).unwrap_or_default()
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
	
	pub fn list_collections(
		&self,
		filter:  Option<bson::Document>,
		options: Option<ListCollectionsOptions>
	) -> Result<Cursor> {
		let options = options.unwrap_or_default();
		self.run_cursor_command(&ListCollectionsCommand {
			list_collections:       1,
			filter,
			name_only:              options.name_only,
			authorized_collections: options.authorized_collections
		},
		None,
		CursorOptions {
			batch_size:      options.batch_size,
			max_time_ms:     options.max_time_ms,
			read_preference: Some(self.0.read_preference.clone())
		})
	}
	
	pub fn drop(self) -> Result<()> {
		self.run_command(&DropDatabaseCommand {
			drop_database: &self.0.name,
			write_concern: self.0.write_concern
		}, Some(&self.0.read_preference), true)
	}
}

pub struct CursorOptions {
	pub batch_size:      Option<i64>,
	pub max_time_ms:     Option<i64>,
	pub read_preference: Option<ReadPreference>
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthCommand<'a> {
	pub authenticate: i32,
	pub user:         &'a str,
	pub pwd:          &'a str
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListCollectionsCommand {
	list_collections:       i32,
	#[serde(skip_serializing_if = "Option::is_none")]
	filter:                 Option<bson::Document>,
	#[serde(skip_serializing_if = "Option::is_none")]
	name_only:              Option<bool>,
	#[serde(skip_serializing_if = "Option::is_none")]
	authorized_collections: Option<bool>
}

#[derive(Debug, Default, Copy, Clone)]
pub struct ListCollectionsOptions {
	pub name_only:              Option<bool>,
	pub authorized_collections: Option<bool>,
	pub batch_size:             Option<i64>,
	pub max_time_ms:            Option<i64>
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DropDatabaseCommand<'a> {
	pub drop_database: &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern: Option<WriteConcern>
}