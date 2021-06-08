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
	serde::{Serialize, Deserialize}
};

#[derive(Debug, Clone)]
pub struct Collection {
	pub db:              DataBase,
	pub name:            String,
	pub read_preference: ReadPreference,
	pub read_concern:    Option<ReadConcern>,
	pub write_concern:   Option<WriteConcern>
}

impl Collection {
	/// Runs an aggregation framework pipeline.
	///
	/// see https://docs.mongodb.com/manual/reference/command/aggregate/
	pub fn aggregate(
		&self,
		pipeline: &[bson::Document],
		options:  Option<AggregateOptions>
	) -> Result<Cursor> {
		let options = options.unwrap_or_default();
		self.db.run_cursor_command(&AggregateCommand {
			aggregate:                  self.name.as_str(),
			pipeline,
			explain:                    None,
			allow_disk_use:             options.allow_disk_usage,
			cursor:                     Some(AggregateCommandCursor { batch_size: options.batch_size }),
			max_time_ms:                options.max_time_ms,
			bypass_document_validation: options.bypass_document_validation,
			read_concern:               self.read_concern,
			collation:                  options.collation,
			hint:                       options.hint,
			comment:                    options.comment.as_deref(),
			write_concern:              self.write_concern
		},
	   None,
	   CursorOptions {
		   batch_size:      options.batch_size,
		   max_time_ms:     options.max_time_ms,
		   read_preference: Some(self.read_preference.clone())
		})
	}
	
	/// Count the number of documents in a collection that match the given
	/// filter. Note that an empty filter will force a scan of the entire
	/// collection. For a fast count of the total documents in a collection
	/// see `estimated_document_count`.
	pub fn count_documents(
		&self,
		filter:  bson::Document,
		options: Option<CountOptions>
	) -> Result<usize> {
		#[derive(Deserialize)]
		struct CountDocumentsResult { n: i32 }
		
		let mut pipeline = vec![doc! { "$match" => filter }];
		let mut aggregate_options = None;
		
		if let Some(options) = options {
			if let Some(skip) = options.skip {
				pipeline.push(doc! { "$skip" => skip });
			}
			
			if let Some(limit) = options.limit {
				pipeline.push(doc! { "limit" => limit });
			}
			
			if options.collation.is_some() || options.hint.is_some()
				|| options.max_time_ms.is_some() {
				aggregate_options = Some(AggregateOptions {
					collation:   options.collation,
					max_time_ms: options.max_time_ms,
					hint:        options.hint,
					..AggregateOptions::default()
				})
			}
		}
		
		pipeline.push(doc! {"$group" => doc! { "_id" => 1, "n" => doc! { "$sum" => 1 } } });
		
		Ok(self.aggregate(&pipeline, aggregate_options)?
			.next()
			.transpose()?
			.map(Document::into::<CountDocumentsResult>)
			.transpose()?
			.map_or(0, |r| r.n as usize))
	}
	
	/// Gets an estimate of the count of documents in a collection using collection metadata.
	pub fn estimated_document_count(
		&self,
		_options: Option<EstimatedDocumentCountOptions>
	) -> Result<usize> {
		self.db.run_command(&CountCommand {
			count:        &self.name,
			query:        None,
			limit:        None,
			skip:         None,
			hint:         None,
			read_concern: self.read_concern
		}, Some(&self.read_preference), true)
			.map(|result: WriteCommandResponse| result.n as _)
	}
	
	/// Finds the distinct values for a specified field across a single collection.
	///
	/// see https://docs.mongodb.com/manual/reference/command/distinct/
	pub fn distinct(
		&self,
		field_name:  &str,
		filter:      Option<bson::Document>,
		mut options: Option<DistinctOptions>
	) -> Result<Vec<bson::Document>> {
		self.db.run_command(&DistinctCommand {
			distinct:     &self.name,
			key:          field_name,
			query:        filter,
			read_concern: self.read_concern,
			collation:    options.as_mut().and_then(|o| o.collation.take())
		}, Some(&self.read_preference), true)
			.map(|result: DistinctCommandResult| result.values)
	}
	
	pub fn find_one(
		&self,
		filter:  Option<bson::Document>,
		options: Option<FindOptions>
	) -> Result<Option<bson::Document>> {
		let mut options = options.unwrap_or_default();
		options.batch_size = Some(1);
		options.limit = Some(1);
		self.find(filter, Some(options)).and_then(|mut c| c.next().transpose())
	}
	
	pub fn find(
		&self,
		filter:  Option<bson::Document>,
		options: Option<FindOptions>
	) -> Result<Cursor> {
		let options = options.unwrap_or_default();
		self.db.run_cursor_command(&FindCommand {
			find:                  &self.name,
			filter,
			sort:                  options.sort,
			projection:            options.projection,
			hint:                  options.hint,
			skip:                  options.skip,
			limit:                 options.limit,
			batch_size:            options.batch_size,
			single_batch:          None,
			comment:               options.comment.as_deref(),
			max_time_ms:           options.max_time_ms,
			read_concern:          self.read_concern,
			max:                   options.max,
			min:                   options.min,
			return_key:            options.return_key,
			show_record_id:        options.show_record_id,
			tailable:              None,
			oplog_replay:          None,
			no_cursor_timeout:     options.no_cursor_timeout,
			await_data:            None,
			allow_partial_results: options.allow_partial_results,
			collation:             options.collation
		},
		Some(&self.read_preference),
		CursorOptions {
			batch_size:      options.batch_size,
			max_time_ms:     options.max_time_ms,
			read_preference: Some(self.read_preference.clone())
		})
	}
	
	/// Executes multiple write operations.
	pub fn bulk_write(
		&self,
		_requests: &[WriteModel],
		_options:  Option<BulkWriteOptions>
	) -> Result<BulkWriteResult> {
		unimplemented!() // TODO bulk write
	}
	
	/// Inserts the provided document.
	///
	/// see https://docs.mongodb.com/manual/reference/command/insert/
	pub fn insert_one(
		&self,
		mut document: bson::Document,
		options:      Option<InsertOneOptions>
	) -> Result<InsertOneResult> {
		let inserted_id = match &options {
			Some(InsertOneOptions { __generate_id: Some(true), .. }) => {
				let id = crate::bson::oid::ObjectId::new();
				document.as_mut().insert("_id", &id);
				Some(id)
			}
			_ => None
		};
		
		self.db.run_command(&InsertCommand {
			insert:                     &self.name,
			documents:                  &[document],
			ordered:                    None,
			write_concern:              self.write_concern,
			bypass_document_validation: options.as_ref()
				.and_then(|o| o.bypass_document_validation)
		}, None, true)
			.and_then(WriteCommandResponse::into_result)
			.map(|_| InsertOneResult {
				acknowledged: true,
				inserted_id
			})
	}
	
	/// Inserts the provided documents.
	///
	/// see https://docs.mongodb.com/manual/reference/command/insert/
	pub fn insert_many(
		&self,
		documents: impl IntoIterator<Item = bson::Document>,
		options:   Option<InsertManyOptions>
	) -> Result<InsertManyResult> {
		let (docs, inserted_ids) = match &options {
			Some(InsertManyOptions { __generate_ids: Some(true), .. }) => {
				let mut ids = Vec::new();
				(documents.into_iter().map(|mut doc| {
					let id = crate::bson::oid::ObjectId::new();
					ids.push(id);
					doc.as_mut().insert("_id", &id);
					doc
				}).collect::<Vec<_>>(), Some(ids))
			}
			_ => (documents.into_iter().collect::<Vec<_>>(), None)
		};
		
		self.db.run_command(&InsertCommand {
			insert:                     &self.name,
			documents:                  &docs,
			ordered:                    options.as_ref().map(|o| o.ordered),
			write_concern:              self.write_concern,
			bypass_document_validation: options.as_ref()
				.and_then(|o| o.bypass_document_validation)
		}, None, true)
			.and_then(WriteCommandResponse::into_result)
			.map(|_| InsertManyResult {
				acknowledged: true,
				inserted_ids
			})
	}
	
	/// Deletes one document.
	///
	/// see https://docs.mongodb.com/manual/reference/command/delete/
	pub fn delete_one(
		&self,
		filter:      bson::Document,
		mut options: Option<DeleteOptions>
	) -> Result<DeleteResult> {
		self.db.run_command(&DeleteCommand {
			delete:        &self.name,
			deletes:       &[Delete {
				q:         filter,
				limit:     1,
				collation: options.as_mut()
					.and_then(|o| o.collation.take())
			}],
			ordered:       None,
			write_concern: self.write_concern
		}, None, true)
			.and_then(WriteCommandResponse::into_result)
			.map(|result| DeleteResult {
				acknowledged:  true,
				deleted_count: result.n as _
			})
	}
	
	/// Deletes multiple documents.
	///
	/// see https://docs.mongodb.com/manual/reference/command/delete/
	pub fn delete_many(
		&self,
		filter:      bson::Document,
		mut options: Option<DeleteOptions>
	) -> Result<DeleteResult> {
		self.db.run_command(&DeleteCommand {
			delete:        &self.name,
			deletes:       &[Delete {
				q:         filter,
				limit:     0,
				collation: options.as_mut().and_then(|o| o.collation.take())
			}],
			ordered:       None,
			write_concern: self.write_concern
		}, None, true)
			.and_then(WriteCommandResponse::into_result)
			.map(|result| DeleteResult {
				acknowledged:  true,
				deleted_count: result.n as _
			})
	}
	
	/// Replaces a single document.
	///
	/// see https://docs.mongodb.com/manual/reference/command/update/
	pub fn replace_one(
		&self,
		filter:      bson::Document,
		replacement: bson::Document,
		options:     Option<ReplaceOptions>
	) -> Result<UpdateResult> {
		self.update_one(filter, replacement, options
			.map(|o| UpdateOptions {
				array_filters:              None,
				bypass_document_validation: o.bypass_document_validation,
				collation:                  o.collation,
				upsert:                     o.upsert
			}))
	}
	
	/// Updates one document.
	///
	/// see https://docs.mongodb.com/manual/reference/command/update/
	pub fn update_one(
		&self,
		filter:      bson::Document,
		update:      bson::Document,
		mut options: Option<UpdateOptions>
	) -> Result<UpdateResult> {
		self.db.run_command(&UpdateCommand {
			update:                     &self.name,
			updates:                    &[Update {
				q:             filter,
				u:             Some(update),
				pipeline:      None,
				upsert:        options.as_ref().and_then(|o| o.upsert),
				multi:         Some(false),
				collation:     options.as_mut().and_then(|o| o.collation.take()),
				array_filters: options.as_ref()
					.and_then(|o| o.array_filters.as_deref())
			}],
			ordered:                    None,
			write_concern:              self.write_concern,
			bypass_document_validation: options.as_ref()
				.and_then(|o| o.bypass_document_validation)
		}, None, true)
			.and_then(WriteCommandResponse::into_result)
			.map(|result| UpdateResult {
				acknowledged:   true,
				matched_count:  result.n as _,
				modified_count: result.n_modified.unwrap_or(0) as _,
				upserted_id:    result.upserted.and_then(|v|
					v.first().map(|u| u._id))
			})
	}
	
	/// Updates one document.
	///
	/// see https://docs.mongodb.com/manual/reference/command/update/
	pub fn update_one_pipeline(
		&self,
		filter:      bson::Document,
		pipeline:    &[bson::Document],
		mut options: Option<UpdateOptions>
	) -> Result<UpdateResult> {
		self.db.run_command(&UpdateCommand {
			update:                     &self.name,
			updates:                    &[Update {
				q:             filter,
				u:             None,
				pipeline:      Some(pipeline),
				upsert:        options.as_ref().and_then(|o| o.upsert),
				multi:         Some(false),
				collation:     options.as_mut().and_then(|o| o.collation.take()),
				array_filters: options.as_ref()
					.and_then(|o| o.array_filters.as_deref())
			}],
			ordered:                    None,
			write_concern:              self.write_concern,
			bypass_document_validation: options.as_ref()
				.and_then(|o| o.bypass_document_validation)
		}, None, true)
			.and_then(WriteCommandResponse::into_result)
			.map(|result| UpdateResult {
				acknowledged:   true,
				matched_count:  result.n as _,
				modified_count: result.n_modified.unwrap_or(0) as _,
				upserted_id:    result.upserted.and_then(|v|
					v.first().map(|u| u._id))
			})
	}
	
	/// Updates multiple documents.
	///
	/// see https://docs.mongodb.com/manual/reference/command/update/
	pub fn update_many(
		&self,
		filter:      bson::Document,
		update:      bson::Document,
		mut options: Option<UpdateOptions>
	) -> Result<UpdateResult> {
		self.db.run_command(&UpdateCommand {
			update:                     &self.name,
			updates:                    &[Update {
				q:             filter,
				u:             Some(update),
				pipeline:      None,
				upsert:        options.as_ref().and_then(|o| o.upsert),
				multi:         Some(true),
				collation:     options.as_mut().and_then(|o| o.collation.take()),
				array_filters: options.as_ref()
					.and_then(|o| o.array_filters.as_deref())
			}],
			ordered:                    None,
			write_concern:              self.write_concern,
			bypass_document_validation: options.as_ref()
				.and_then(|o| o.bypass_document_validation)
		}, None, true)
			.and_then(WriteCommandResponse::into_result)
			.map(|result| UpdateResult {
				acknowledged:   true,
				matched_count:  result.n as _,
				modified_count: result.n_modified.unwrap_or(0) as _,
				upserted_id:    result.upserted.and_then(|v|
					v.first().map(|u| u._id))
			})
	}
	
	/// Finds a single document and deletes it, returning the original. The document to return may be null.
	///
	/// see https://docs.mongodb.com/manual/reference/command/findAndModify/
	pub fn find_one_and_delete(
		&self,
		filter:  bson::Document,
		options: Option<FindOneAndDeleteOptions>
	) -> Result<Option<bson::Document>> {
		let options = options.unwrap_or_default();
		self.db.run_command(&FindAndModifyCommand {
			find_and_modify:            &self.name,
			remove:                     Some(true),
			update:                     None,
			query:                      Some(filter),
			sort:                       options.sort,
			new:                        None,
			fields:                     options.projection,
			upsert:                     None,
			bypass_document_validation: None,
			write_concern:              self.write_concern,
			max_time_ms:                options.max_time_ms,
			collation:                  options.collation,
			array_filters:              None
		}, None, true)
			.map(|result: FindAndModifyCommandResult| result.value)
	}
	
	/// Finds a single document and replaces it, returning either the original or the replaced
	/// document. The document to return may be null.
	///
	/// see https://docs.mongodb.com/manual/reference/command/findAndModify/
	pub fn find_one_and_replace(
		&self,
		filter:      bson::Document,
		replacement: bson::Document,
		options:     Option<FindOneAndReplaceOptions>
	) -> Result<Option<bson::Document>> {
		let options = options.unwrap_or_default();
		self.db.run_command(&FindAndModifyCommand {
			find_and_modify:            &self.name,
			remove:                     None,
			update:                     Some(replacement),
			query:                      Some(filter),
			sort:                       options.sort,
			new:                        options.return_document.map(|v| match v {
				ReturnDocument::Before => false,
				ReturnDocument::After  => true
			}),
			fields:                     options.projection,
			upsert:                     options.upsert,
			bypass_document_validation: options.bypass_document_validation,
			write_concern:              self.write_concern,
			max_time_ms:                options.max_time_ms,
			collation:                  options.collation,
			array_filters:              None
		}, None, true)
			.map(|result: FindAndModifyCommandResult| result.value)
	}
	
	/// Finds a single document and updates it, returning either the original or the updated
	/// document. The document to return may be null.
	///
	/// see https://docs.mongodb.com/manual/reference/command/findAndModify/
	pub fn find_one_and_update(
		&self,
		filter:  bson::Document,
		update:  bson::Document,
		options: Option<FindOneAndUpdateOptions>
	) -> Result<Option<bson::Document>> {
		let options = options.unwrap_or_default();
		self.db.run_command(&FindAndModifyCommand {
			find_and_modify:            &self.name,
			remove:                     None,
			update:                     Some(update),
			query:                      Some(filter),
			sort:                       options.sort,
			new:                        options.return_document.map(|v| match v {
				ReturnDocument::Before => false,
				ReturnDocument::After  => true
			}),
			fields:                     options.projection,
			upsert:                     options.upsert,
			bypass_document_validation: options.bypass_document_validation,
			write_concern:              self.write_concern,
			max_time_ms:                options.max_time_ms,
			collation:                  options.collation,
			array_filters:              options.array_filters.as_deref()
		}, None, true)
			.map(|result: FindAndModifyCommandResult| result.value)
	}
	
	/// Returns a change stream on a specific collection.
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
	
	pub fn rename(
		&self,
		name:    &str,
		options: Option<RenameCollectionOptions>
	) -> Result<()> {
		let options = options.unwrap_or_default();
		self.db.run_command(&RenameCollectionCommand {
			rename_collection: &format!("{}.{}", &self.db.0.name, &self.name),
			to:                name,
			drop_target:       options.drop_target,
			write_concern:     self.write_concern
		}, None, true)
	}
	
	pub fn drop(&self) -> Result<()> {
		self.db.run_command(&DropCommand {
			drop:          &self.name,
			write_concern: self.write_concern
		}, None, true)
	}
	
	pub fn create_index(
		&self,
		keys:    &[(&str, &str)],
		options: Option<IndexOptions>
	) -> Result<()> {
		self.create_indexes(Some((keys, options)).into_iter())
	}
	
	pub fn create_indexes<'a>(
		&self,
		iter: impl IntoIterator<Item = (&'a [(&'a str, &'a str)], Option<IndexOptions<'a>>)>
	) -> Result<()> {
		self.db.run_command(&CreateIndexesCommand {
			create_indexes: &self.name,
			indexes:        &iter.into_iter().map(|(key, options)| {
				let options = options.unwrap_or_default();
				CreateIndexesCommandIndex {
					key,
					name:                      options.name,
					unique:                    options.unique,
					partial_filter_expression: options.partial_filter_expression,
					sparse:                    options.sparse,
					expire_after_seconds:      options.expire_after_seconds,
					storage_engine:            options.storage_engine,
					weights:                   options.weights,
					default_language:          options.default_language,
					language_override:         options.language_override,
					text_index_version:        options.text_index_version,
					_2d_sphere_index_version:  options._2d_sphere_index_version,
					bits:                      options.bits,
					min:                       options.min,
					max:                       options.max,
					bucket_size:               options.bucket_size,
					collation:                 options.collation,
					wildcard_projection:       None
				}
			}).collect::<Vec<_>>(),
			write_concern: self.write_concern
		}, None, true)
			.map(|_: CreateIndexesCommandResult| ())
	}
	
	pub fn list_indexes(&self) -> Result<Cursor> {
		self.db.run_cursor_command(&ListIndexesCommand {
			list_indexes: &self.name
		},
	   Some(&self.read_preference),
	   CursorOptions {
		   batch_size:      None,
		   max_time_ms:     None,
		   read_preference: Some(self.read_preference.clone())
	   })
	}
	
	pub fn drop_index(
		&self,
		name: &str
	) -> Result<()> {
		debug_assert_ne!(name, "*");
		self.db.run_command(&DropIndexesCommand {
			drop_indexes:  &self.name,
			index:         name,
			write_concern: self.write_concern
		}, None, true)
	}
	
	pub fn drop_index_with(
		&self,
		keys:    &[(&str, &str)],
		options: Option<IndexOptions>
	) -> Result<()> {
		#[derive(Serialize)]
		struct IndexOptionsWithKey<'a> {
			#[serde(flatten)]
			options: IndexOptions<'a>,
			key:     &'a [(&'a str, &'a str)]
		}
		
		self.db.run_command(&DropIndexesCommand {
			drop_indexes:  &self.name,
			index:         IndexOptionsWithKey {
				options: options.unwrap_or_default(),
				key:     keys
			},
			write_concern: self.write_concern
		}, None, true)
	}
	
	pub fn drop_indexes(&self) -> Result<()> {
		self.db.run_command(&DropIndexesCommand {
			drop_indexes:  &self.name,
			index:         "*",
			write_concern: self.write_concern
		}, None, true)
	}
}

//
// OPTIONS AND RESULTS
//

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CountOptions {
	/// Specifies a collation.
	/// For servers < 3.4, the driver MUST raise an error if the caller explicitly provides a value.
	pub collation:   Option<Collation>,
	/// The index to use.
	pub hint:        Option<Document>,
	/// The maximum number of documents to count.
	pub limit:       Option<i64>,
	/// The maximum amount of time to allow the operation to run.
	pub max_time_ms: Option<i64>,
	/// The number of documents to skip before counting.
	pub skip:        Option<i64>
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct EstimatedDocumentCountOptions {
	/// The maximum amount of time to allow the operation to run.
	pub max_time_ms: Option<i64>
}

/// see https://docs.mongodb.com/manual/reference/command/distinct/
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct DistinctOptions {
	/// Specifies a collation.
	/// For servers < 3.4, the driver MUST raise an error if the caller explicitly provides a value.
	pub collation:   Option<Collation>,
	/// The maximum amount of time to allow the query to run.
	pub max_time_ms: Option<i64>
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum CursorType {
	/// The default value. A vast majority of cursors will be of this type.
	NonTailable,
	/// Tailable means the cursor is not closed when the last data is retrieved.
	/// Rather, the cursor marks the final object’s position. You can resume
	/// using the cursor later, from where it was located, if more data were
	/// received. Like any “latent cursor”, the cursor may become invalid at
	/// some point (CursorNotFound) – for example if the final object it
	/// references were deleted.
	Tailable,
	/// Combines the tailable option with awaitData, as defined below.
	///
	/// Use with TailableCursor. If we are at the end of the data, block for a
	/// while rather than returning no data. After a timeout period, we do return
	/// as normal. The default is true.
	TailableAwait
}

/// see https://docs.mongodb.com/manual/reference/command/find/
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FindOptions {
	/// Get partial results from a mongos if some shards are down (instead of throwing an error).
	pub allow_partial_results: Option<bool>,
	/// The number of documents to return per batch.
	pub batch_size:            Option<i64>,
	/// Specifies a collation.
	pub collation:             Option<Collation>,
	/// Attaches a comment to the query.
	pub comment:               Option<String>,
	/// Indicates the type of cursor to use. This value includes both
	/// the tailable and awaitData options.
	pub cursor_type:           Option<CursorType>,
	/// The index to use.
	pub hint:                  Option<Document>,
	/// The maximum number of documents to return.
	pub limit:                 Option<i64>,
	/// The exclusive upper bound for a specific index.
	pub max:                   Option<Document>,
	/// The maximum amount of time for the server to wait on new documents to satisfy a tailable cursor
	/// query. This only applies to a TAILABLE_AWAIT cursor. When the cursor is not a TAILABLE_AWAIT cursor,
	/// this option is ignored.
	pub max_await_time_ms:     Option<i64>,
	/// Maximum number of documents or index keys to scan when executing the query.
	pub max_scan:              Option<i64>,
	/// The maximum amount of time to allow the query to run.
	pub max_time_ms:           Option<i64>,
	/// The inclusive lower bound for a specific index.
	pub min:                   Option<Document>,
	/// The server normally times out idle cursors after an inactivity period (10 minutes)
	/// to prevent excess memory use. Set this option to prevent that.
	pub no_cursor_timeout:     Option<bool>,
	/// Limits the fields to return for all matching documents.
	pub projection:            Option<Document>,
	/// If true, returns only the index keys in the resulting documents.
	pub return_key:            Option<bool>,
	/// Determines whether to return the record identifier for each document. If true, adds a field $recordId to the returned documents.
	pub show_record_id:        Option<bool>,
	/// The number of documents to skip before returning.
	pub skip:                  Option<i64>,
	/// The order in which to return matching documents.
	pub sort:                  Option<Document>
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct BulkWriteOptions {
	/// If true, when a write fails, return without performing the remaining
	/// writes. If false, when a write fails, continue with the remaining writes, if any.
	/// Defaults to true.
	pub ordered:                    bool,
	/// If true, allows the write to opt-out of document level validation.
	pub bypass_document_validation: Option<bool>,
}

impl Default for BulkWriteOptions {
	fn default() -> Self {
		Self {
			ordered:                    true,
			bypass_document_validation: None
		}
	}
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct InsertOneOptions {
	/// If true, allows the write to opt-out of document level validation.
	pub bypass_document_validation: Option<bool>,
	/// If true, the object ids will be generated by the driver, rather than the server
	pub __generate_id:             Option<bool>
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct InsertManyOptions {
	/// If true, when an insert fails, return without performing the remaining
	/// writes. If false, when a write fails, continue with the remaining writes, if any.
	/// Defaults to true.
	pub ordered:                    bool,
	/// If true, allows the write to opt-out of document level validation.
	pub bypass_document_validation: Option<bool>,
	/// If true, the object ids will be generated by the driver, rather than the server
	pub __generate_ids:             Option<bool>
}

impl Default for InsertManyOptions {
	fn default() -> Self {
		Self {
			ordered:                    true,
			bypass_document_validation: None,
			__generate_ids:             None
		}
	}
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct UpdateOptions {
	/// A set of filters specifying to which array elements an update should apply.
	pub array_filters:              Option<Vec<Document>>,
	/// If true, allows the write to opt-out of document level validation.
	pub bypass_document_validation: Option<bool>,
	/// Specifies a collation.
	pub collation:                  Option<Collation>,
	/// When true, creates a new document if no document matches the query.
	pub upsert:                     Option<bool>
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct ReplaceOptions {
	/// If true, allows the write to opt-out of document level validation.
	pub bypass_document_validation: Option<bool>,
	/// Specifies a collation.
	pub collation:                  Option<Collation>,
	/// When true, creates a new document if no document matches the query.
	pub upsert:                     Option<bool>
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct DeleteOptions {
	/// Specifies a collation.
	pub collation:                  Option<Collation>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WriteModel {
	InsertOne(InsertOneModel),
	DeleteOne(DeleteOneModel),
	DeleteMany(DeleteManyModel),
	ReplaceOne(ReplaceOneModel),
	UpdateOne(UpdateOneModel),
	UpdateMany(UpdateManyModel)
}

#[derive(Debug, Clone, PartialEq)]
pub struct InsertOneModel {
	/// The document to insert.
	pub document: Document
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeleteOneModel {
	/// The filter to limit the deleted documents.
	pub filter:    Document,
	/// Specifies a collation.
	pub collation: Option<Collation>
}

pub type DeleteManyModel = DeleteOneModel;

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceOneModel {
	/// The filter to limit the replaced document.
	pub filter:      Document,
	/// The document with which to replace the matched document.
	pub replacement: Document,
	/// Specifies a collation.
	pub collation:   Option<Collation>,
	/// When true, creates a new document if no document matches the query.
	pub upsert:      Option<bool>
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateOneModel {
	/// The filter to limit the updated documents.
	pub filter:        Document,
	/// A document or pipeline containing update operators.
	pub update:        Vec<Document>,
	/// A set of filters specifying to which array elements an update should apply.
	pub array_filters: Vec<Document>,
	/// Specifies a collation.
	pub collation:     Option<Collation>,
	/// When true, creates a new document if no document matches the query.
	pub upsert:        Option<bool>
}

pub type UpdateManyModel = UpdateOneModel;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BulkWriteResult {
	/// Indicates whether this write result was acknowledged. If not, then all
	/// other members of this result will be undefined.
	pub acknowledged:   bool,
	/// Number of documents inserted.
	pub inserted_count: i64,
	/// Map of the index of the operation to the id of the inserted document.
	pub inserted_ids:   Vec<oid::ObjectId>,
	/// Number of documents matched for update.
	pub matched_count:  i64,
	/// Number of documents modified.
	pub modified_count: i64,
	/// Number of documents deleted.
	pub deleted_count:  i64,
	/// Number of documents upserted.
	pub upserted_ids:   Vec<oid::ObjectId>
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InsertOneResult {
	/// Indicates whether this write result was acknowledged. If not, then all
	/// other members of this result will be undefined.
	pub acknowledged: bool,
	/// The identifier that was inserted. If the server generated the identifier, this value
	/// will be null as the driver does not have access to that data.
	pub inserted_id:  Option<oid::ObjectId>
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct InsertManyResult {
	/// Indicates whether this write result was acknowledged. If not, then all
	/// other members of this result will be undefined.
	pub acknowledged: bool,
	/// Map of the index of the inserted document to the id of the inserted document.
	pub inserted_ids: Option<Vec<oid::ObjectId>>
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeleteResult {
	/// Indicates whether this write result was acknowledged. If not, then all
	/// other members of this result will be undefined.
	pub acknowledged:  bool,
	/// The number of documents that were deleted.
	pub deleted_count: usize
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct UpdateResult {
	/// Indicates whether this write result was acknowledged. If not, then all
	/// other members of this result will be undefined.
	pub acknowledged:   bool,
	/// The number of documents that matched the filter.
	pub matched_count:  usize,
	/// The number of documents that were modified.
	pub modified_count: usize,
	/// The identifier of the inserted document if an upsert took place.
	pub upserted_id:    Option<oid::ObjectId>
}

#[derive(Debug, Clone, PartialEq)]
pub struct WriteConcernError {
	/// An integer value identifying the write concern error.
	pub code:    i32,
	/// A document identifying the write concern setting related to the error.
	pub details: Document,
	/// A description of the error.
	pub message: String
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WriteError {
	/// An integer value identifying the error.
	pub code:    i32,
	/// A description of the error.
	pub message: String
}

#[derive(Debug, Clone, PartialEq)]
pub struct BulkWriteError {
	/// An integer value identifying the error.
	pub code:    i32,
	/// A description of the error.
	pub message: String,
	/// The index of the request that errored.
	pub index:   i32,
	/// The request that errored.
	pub request: Option<WriteModel>
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ReturnDocument {
	/// Indicates to return the document before the update, replacement, or insert occured.
	Before,
	/// Indicates to return the document after the update, replacement, or insert occured.
	After
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct FindOneAndDeleteOptions {
	/// Specifies a collation.
	pub collation:   Option<Collation>,
	/// The maximum amount of time to allow the query to run.
	pub max_time_ms: Option<i32>,
	/// Limits the fields to return for all matching documents.
	pub projection:  Option<Document>,
	/// Determines which document the operation modifies if the query selects multiple documents.
	pub sort:        Option<Document>
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct FindOneAndReplaceOptions {
	/// If true, allows the write to opt-out of document level validation.
	pub bypass_document_validation: Option<bool>,
	/// Specifies a collation.
	pub collation:                  Option<Collation>,
	/// The maximum amount of time to allow the query to run.
	pub max_time_ms:                Option<i32>,
	/// Limits the fields to return for all matching documents.
	pub projection:                 Option<Document>,
	/// When ReturnDocument.After, returns the replaced or inserted document rather than the original.
	pub return_document:            Option<ReturnDocument>,
	/// Determines which document the operation modifies if the query selects multiple documents.
	pub sort:                       Option<Document>,
	/// When true, findAndModify creates a new document if no document matches the query.
	pub upsert:                     Option<bool>
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct FindOneAndUpdateOptions {
	/// A set of filters specifying to which array elements an update should apply.
	pub array_filters:              Option<Vec<Document>>,
	/// If true, allows the write to opt-out of document level validation.
	pub bypass_document_validation: Option<bool>,
	/// Specifies a collation.
	pub collation:                  Option<Collation>,
	/// The maximum amount of time to allow the query to run.
	pub max_time_ms:                Option<i32>,
	/// Limits the fields to return for all matching documents.
	pub projection:                 Option<Document>,
	/// When ReturnDocument.After, returns the replaced or inserted document rather than the original.
	pub return_document:            Option<ReturnDocument>,
	/// Determines which document the operation modifies if the query selects multiple documents.
	pub sort:                       Option<Document>,
	/// When true, creates a new document if no document matches the query.
	pub upsert:                     Option<bool>
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexOptions<'a> {
	#[serde(skip_serializing_if = "Option::is_some")]
	pub expire_after_seconds:      Option<i32>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub name:                      Option<&'a str>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub sparse:                    Option<bool>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub storage_engine:            Option<Document>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub unique:                    Option<bool>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub version:                   Option<i32>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub default_language:          Option<&'a str>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub language_override:         Option<&'a str>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub text_index_version:        Option<i32>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub weights:                   Option<Document>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub _2d_sphere_index_version:  Option<i32>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub bits:                      Option<u32>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub max:                       Option<f64>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub min:                       Option<f64>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub bucket_size:               Option<i32>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub partial_filter_expression: Option<Document>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub collation:                 Option<Collation>,
	#[serde(skip_serializing_if = "Option::is_some")]
	pub wildcard_projection:       Option<Document>
}

//
// INTERNAL COMMANDS
//

/// Performs aggregation operation using the aggregation pipeline. The pipeline allows users to
/// process data from a collection or other source with a sequence of stage-based manipulations.
///
/// https://docs.mongodb.com/manual/reference/command/aggregate/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateCommand<'a, T: BsonValue<'a>> {
	/// The name of the collection or view that acts as the input for the aggregation pipeline.
	/// Use 1 for collection agnostic commands.
	pub aggregate:                  T,
	/// An array of aggregation pipeline stages that process and transform the document stream
	/// as part of the aggregation pipeline.
	pub pipeline:                   &'a [bson::Document],
	/// Optional. Specifies to return the information on the processing of the pipeline.
	///
	/// Not available in multi-document transactions.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub explain:                    Option<bool>,
	/// Optional. Enables writing to temporary files. When set to true, aggregation stages
	/// can write data to the _tmp subdirectory in the dbPath directory.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub allow_disk_use:             Option<bool>,
	/// Specify a document that contains options that control the creation of the cursor object.
	pub cursor:                     Option<AggregateCommandCursor>,
	#[serde(skip_serializing_if = "Option::is_none")]
	/// Optional. Specifies a time limit in milliseconds for processing operations on a cursor.
	/// If you do not specify a value for maxTimeMS, operations will not time out. A value of 0
	/// explicitly specifies the default unbounded behavior.
	///
	/// MongoDB terminates operations that exceed their allotted time limit using the same
	/// mechanism as db.killOp(). MongoDB only terminates an operation at one of its designated
	/// interrupt points.
	pub max_time_ms:                Option<i64>,
	/// Optional. Available only if you specify the $out aggregation operator.
	///
	/// Enables aggregate to bypass document validation during the operation. This lets you
	/// insert documents that do not meet the validation requirements.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub bypass_document_validation: Option<bool>,
	/// Optional. Specifies the read concern.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub read_concern:               Option<ReadConcern>,
	/// Optional. Specifies the collation to use for the operation.
	///
	/// Collation allows users to specify language-specific rules for string comparison,
	/// such as rules for lettercase and accent marks.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub collation:                  Option<Collation>,
	/// Optional. The index to use for the aggregation. The index is on the initial
	/// collection/view against which the aggregation is run.
	///
	/// Specify the index either by the index name or by the index specification document.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub hint:                       Option<bson::Document>,
	/// Optional. Users can specify an arbitrary string to help trace the operation through
	/// the database profiler, currentOp, and logs.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub comment:                    Option<&'a str>,
	/// Optional. A document that expresses the write concern to use with $out stage.
	///
	/// Omit to use the default write concern with the $out stage.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern:              Option<WriteConcern>
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateCommandCursor {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub batch_size: Option<i64>
}

/// Counts the number of documents in a collection or a view. Returns a document that
/// contains this count and as well as the command status.
///
/// https://docs.mongodb.com/manual/reference/command/count/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountCommand<'a> {
	/// The name of the collection or view to count.
	pub count:        &'a str,
	/// Optional. A query that selects which documents to count in the collection or view.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub query:        Option<bson::Document>,
	/// Optional. The maximum number of matching documents to return.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub limit:        Option<i64>,
	/// Optional. The number of matching documents to skip before returning results.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub skip:         Option<i64>,
	/// Optional. The index to use. Specify either the index name as a string or the
	/// index specification document.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub hint:         Option<bson::Document>,
	/// Optional. Specifies the read concern.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub read_concern: Option<ReadConcern>
}

/// Finds the distinct values for a specified field across a single collection. distinct
/// returns a document that contains an array of the distinct values. The return document
/// also contains an embedded document with query statistics and the query plan.
///
/// https://docs.mongodb.com/manual/reference/command/distinct/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DistinctCommand<'a> {
	/// The name of the collection to query for distinct values.
	pub distinct:     &'a str,
	/// The field for which to return distinct values.
	pub key:          &'a str,
	/// Optional. A query that specifies the documents from which to retrieve the distinct values.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub query:        Option<bson::Document>,
	/// Optional. Specifies the read concern.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub read_concern: Option<ReadConcern>,
	/// Optional. Specifies the collation to use for the operation.
	///
	/// Collation allows users to specify language-specific rules for string comparison,
	/// such as rules for lettercase and accent marks.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub collation:    Option<Collation>
}

#[derive(Debug, Clone, Deserialize)]
pub struct DistinctCommandResult {
	pub values: Vec<bson::Document>
}

/// The insert command inserts one or more documents and returns a document containing the
/// status of all inserts. The insert methods provided by the MongoDB drivers use this
/// command internally.
///
/// https://docs.mongodb.com/manual/reference/command/insert/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertCommand<'a> {
	/// The name of the target collection.
	pub insert:                     &'a str,
	/// An array of one or more documents to insert into the named collection.
	pub documents:                  &'a [bson::Document],
	/// Optional. If true, then when an insert of a document fails, return without inserting
	/// any remaining documents listed in the inserts array. If false, then when an insert
	/// of a document fails, continue to insert the remaining documents. Defaults to true.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub ordered:                    Option<bool>,
	/// Optional. A document that expresses the write concern of the insert command. Omit
	/// to use the default write concern.
	///
	/// Do not explicitly set the write concern for the operation if run in a transaction.
	/// To use write concern with transactions, see Read Concern/Write Concern/Read Preference.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern:              Option<WriteConcern>,
	/// Optional. Enables insert to bypass document validation during the operation. This
	/// lets you insert documents that do not meet the validation requirements.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub bypass_document_validation: Option<bool>
}

/// The update command modifies documents in a collection. A single update command can
/// contain multiple update statements. The update methods provided by the MongoDB drivers
/// use this command internally.
///
/// https://docs.mongodb.com/manual/reference/command/update/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCommand<'a> {
	/// The name of the target collection.
	pub update:                     &'a str,
	/// An array of one or more update statements to perform in the named collection.
	pub updates:                    &'a [Update<'a>],
	/// Optional. If true, then when an update statement fails, return without performing
	/// the remaining update statements. If false, then when an update fails, continue
	/// with the remaining update statements, if any. Defaults to true.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub ordered:                    Option<bool>,
	/// Optional. A document that expresses the write concern of the insert command. Omit
	/// to use the default write concern.
	///
	/// Do not explicitly set the write concern for the operation if run in a transaction.
	/// To use write concern with transactions, see Read Concern/Write Concern/Read Preference.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern:              Option<WriteConcern>,
	/// Optional. Enables update to bypass document validation during the operation. This
	/// lets you update documents that do not meet the validation requirements.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub bypass_document_validation: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update<'a> {
	/// The query that matches documents to update. Use the same query selectors as used
	/// in the find() method.
	pub q:             bson::Document,
	/// The modifications to apply. For details, see Behavior.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub u:             Option<bson::Document>,
	/// The modifications to apply. For details, see Behavior.
	#[serde(rename = "u")]
	#[serde(skip_serializing_if = "Option::is_none")]
	pub pipeline:      Option<&'a [bson::Document]>,
	/// Optional. If true, perform an insert if no documents match the query. If both upsert
	/// and multi are true and no documents match the query, the update operation inserts
	/// only a single document.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub upsert:        Option<bool>,
	/// Optional. If true, updates all documents that meet the query criteria. If false,
	/// limit the update to one document that meet the query criteria. Defaults to false.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub multi:         Option<bool>,
	/// Optional. Specifies the collation to use for the operation.
	///
	/// Collation allows users to specify language-specific rules for string comparison,
	/// such as rules for lettercase and accent marks.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub collation:     Option<Collation>,
	/// Optional. An array of filter documents that determines which array elements to modify
	/// for an update operation on an array field.
	///
	/// In the update document, use the `$[<identifier>]` filtered positional operator to
	/// define an identifier, which you then reference in the array filter documents.
	/// You cannot have an array filter document for an identifier if the identifier
	/// is not included in the update document.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub array_filters: Option<&'a [bson::Document]>
}

/// The delete command removes documents from a collection. A single delete command can
/// contain multiple delete specifications. The command cannot operate on capped collections.
/// The remove methods provided by the MongoDB drivers use this command internally.
///
/// https://docs.mongodb.com/manual/reference/command/delete/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCommand<'a> {
	/// The name of the target collection.
	pub delete:                     &'a str,
	/// An array of one or more delete statements to perform in the named collection.
	pub deletes:                    &'a [Delete],
	/// Optional. If true, then when a delete statement fails, return without performing
	/// the remaining delete statements. If false, then when a delete statement fails,
	/// continue with the remaining delete statements, if any. Defaults to true.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub ordered:                    Option<bool>,
	/// Optional. A document that expresses the write concern of the insert command. Omit
	/// to use the default write concern.
	///
	/// Do not explicitly set the write concern for the operation if run in a transaction.
	/// To use write concern with transactions, see Read Concern/Write Concern/Read Preference.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern:              Option<WriteConcern>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Delete {
	/// The query that matches documents to delete.
	pub q:         bson::Document,
	/// The number of matching documents to delete. Specify either a 0 to delete all
	/// matching documents or 1 to delete a single document.
	pub limit:     i32,
	/// Optional. Specifies the collation to use for the operation.
	///
	/// Collation allows users to specify language-specific rules for string comparison,
	/// such as rules for lettercase and accent marks.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub collation: Option<Collation>
}

/// The findAndModify command modifies and returns a single document. By default, the
/// returned document does not include the modifications made on the update. To return
/// the document with the modifications made on the update, use the new option.
///
/// https://docs.mongodb.com/manual/reference/command/findAndModify/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindAndModifyCommand<'a> {
	///
	pub find_and_modify:            &'a str,
	/// Optional. The selection criteria for the modification. The query field employs the
	/// same query selectors as used in the db.collection.find() method. Although the query
	/// may match multiple documents, findAndModify will only select one document to modify.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub query:                      Option<bson::Document>,
	/// Optional. Determines which document the operation modifies if the query selects
	/// multiple documents. findAndModify modifies the first document in the sort order
	/// specified by this argument.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub sort:                       Option<bson::Document>,
	/// Must specify either the remove or the update field. Removes the document specified
	/// in the query field. Set this to true to remove the selected document. The default
	/// is false.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub remove:                     Option<bool>,
	/// Must specify either the remove or the update field. Performs an update of the
	/// selected document. The update field employs the same update operators or field:
	/// value specifications to modify the selected document.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub update:                     Option<bson::Document>,
	/// Optional. When true, returns the modified document rather than the original. The
	/// findAndModify method ignores the new option for remove operations. The default
	/// is false.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub new:                        Option<bool>,
	/// Optional. A subset of fields to return. The fields document specifies an inclusion
	/// of a field with 1, as in: `fields: { <field1>: 1, <field2>: 1, ... }`. See projection.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub fields:                     Option<bson::Document>,
	/// Optional. Used in conjuction with the update field.
	///
	/// When true, findAndModify() either:
	///
	/// - Creates a new document if no documents match the query. For more details see upsert behavior.
	/// - Updates a single document that matches the query.
	///
	/// To avoid multiple upserts, ensure that the query fields are uniquely indexed.
	///
	/// Defaults to false.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub upsert:                     Option<bool>,
	/// Optional. Enables findAndModify to bypass document validation during the operation.
	/// This lets you update documents that do not meet the validation requirements.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub bypass_document_validation: Option<bool>,
	/// Optional. A document that expresses the write concern of the insert command. Omit
	/// to use the default write concern.
	///
	/// Do not explicitly set the write concern for the operation if run in a transaction.
	/// To use write concern with transactions, see Read Concern/Write Concern/Read Preference.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern:              Option<WriteConcern>,
	/// Optional. Specifies a time limit in milliseconds for processing the operation.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_time_ms:                Option<i32>,
	/// Optional. Specifies the collation to use for the operation.
	///
	/// Collation allows users to specify language-specific rules for string comparison,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub collation:                  Option<Collation>,
	/// Optional. An array of filter documents that determines which array elements to modify
	/// for an update operation on an array field.
	///
	/// In the update document, use the `$[<identifier>]` filtered positional operator to
	/// define an identifier, which you then reference in the array filter documents.
	/// You cannot have an array filter document for an identifier if the identifier
	/// is not included in the update document.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub array_filters:              Option<&'a [bson::Document]>
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindAndModifyCommandResult {
	/// For remove operations, value contains the removed document if the query matches a document.
	/// If the query does not match a document to remove, value contains null.
	///
	/// For update operations, the value embedded document contains the following:
	///
	/// - If the new parameter is not set or is false:
	///     - the pre-modification document if the query matches a document;
	///     - otherwise, null.
	/// - If new is true:
	///     - the modified document if the query returns a match;
	///     - the inserted document if upsert: true and no document matches the query;
	///     - otherwise, null.
	pub value:             Option<bson::Document>,
	/// Contains information about updated documents. See lastErrorObject for details.
	pub last_error_object: FindAndModifyCommandResultLastErrorObject
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindAndModifyCommandResultLastErrorObject {
	/// Contains true if an update operation modified an existing document.
	pub updated_existing: bool,
	pub updated:          Option<ObjectId>,
	pub n:                Option<i32>
}

/// Executes a query and returns the first batch of results and the cursor id, from which the
/// client can construct a cursor.
///
/// https://docs.mongodb.com/manual/reference/command/find/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindCommand<'a> {
	/// The name of the collection or view to query.
	pub find:                  &'a str,
	/// Optional. The query predicate. If unspecified, then all documents in the collection will
	/// match the predicate.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub filter:                Option<bson::Document>,
	/// Optional. The sort specification for the ordering of the results.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub sort:                  Option<bson::Document>,
	/// Optional. The projection specification to determine which fields to include in the returned documents. See Project Fields to Return from Query and Projection Operators.
	///
	/// `find()` operations on views do not support the following projection operators:
	///
	/// - `$`
	/// - `$elemMatch`
	/// - `$slice`
	/// - `$meta`
	#[serde(skip_serializing_if = "Option::is_none")]
	pub projection:            Option<bson::Document>,
	/// Optional. Index specification. Specify either the index name as a string or the index key
	/// pattern. If specified, then the query system will only consider plans using the hinted index.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub hint:                  Option<bson::Document>,
	/// Optional. Number of documents to skip. Defaults to 0.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub skip:                  Option<i64>,
	/// Optional. The maximum number of documents to return. If unspecified, then defaults to no
	/// limit. A limit of 0 is equivalent to setting no limit.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub limit:                 Option<i64>,
	/// Optional. The number of documents to return in the first batch. Defaults to 101. A
	/// batchSize of 0 means that the cursor will be established, but no documents will be
	/// returned in the first batch.
	///
	/// Unlike the previous wire protocol version, a batchSize of 1 for the find command does not close the cursor.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub batch_size:            Option<i64>,
	/// Optional. Determines whether to close the cursor after the first batch. Defaults to false.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub single_batch:          Option<bool>,
	/// Optional. A comment to attach to the query to help interpret and trace query `profile` data.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub comment:               Option<&'a str>,
	/// Optional. The cumulative time limit in milliseconds for processing operations on the cursor.
	/// MongoDB aborts the operation at the earliest following interrupt point.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_time_ms:           Option<i64>,
	/// Optional. Specifies the read concern.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub read_concern:          Option<ReadConcern>,
	/// Optional. The exclusive upper bound for a specific index. See `cursor.max()` for details.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max:                   Option<bson::Document>,
	/// Optional. The inclusive lower bound for a specific index. See `cursor.min()` for details.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub min:                   Option<bson::Document>,
	/// Optional. If true, returns only the index keys in the resulting documents. Default value
	/// is false. If returnKey is true and the `find` command does not use an index, the returned
	/// documents will be empty.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub return_key:            Option<bool>,
	/// Optional. Determines whether to return the record identifier for each document. If true,
	/// adds a field `$recordId` to the returned documents.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub show_record_id:        Option<bool>,
	/// Optional. Returns a tailable cursor for a capped collections.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tailable:              Option<bool>,
	/// Optional. An internal command for replaying a replica set’s oplog.
	///
	/// To use `oplogReplay`, the find field must refer to a capped collection and you must provide
	/// a `filter` option comparing the ts document field to a `timestamp` using one of the following
	/// comparison operators:
	///
	/// - `$gte`
	/// - `$gt`
	/// - `$eq`
	#[serde(skip_serializing_if = "Option::is_none")]
	pub oplog_replay:          Option<bool>,
	/// Optional. Prevents the server from timing out idle cursors after an inactivity period
	/// (10 minutes).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub no_cursor_timeout:     Option<bool>,
	/// Optional. Use in conjunction with the tailable option to block a `getMore` command on the
	/// cursor temporarily if at the end of data rather than returning no data. After a timeout
	/// period, `find` returns as normal.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub await_data:            Option<bool>,
	/// Optional. For queries against a sharded collection, returns partial results from the mongos
	/// if some shards are unavailable instead of throwing an error.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub allow_partial_results: Option<bool>,
	/// Optional. Specifies the collation to use for the operation.
	///
	/// Collation allows users to specify language-specific rules for string comparison,
	/// such as rules for lettercase and accent marks.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub collation:             Option<Collation>
}

/// Changes the name of an existing collection. Specify collection names to renameCollection in
/// the form of a complete namespace (`<database>.<collection>`).
///
/// Issue the renameCollection command against the admin database.
///
/// https://docs.mongodb.com/manual/reference/command/renameCollection/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameCollectionCommand<'a> {
	/// The namespace of the collection to rename. The namespace is a combination of the database
	/// name and the name of the collection.
	pub rename_collection: &'a str,
	/// The new namespace of the collection. If the new namespace specifies a different database,
	/// the renameCollection command copies the collection to the new database and drops the source
	/// collection.
	pub to:                &'a str,
	/// Optional. If true, mongod will drop the target of renameCollection prior to renaming the
	/// collection. The default value is false.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub drop_target:       Option<bool>,
	/// Optional. A document that expresses the write concern for the operation. Omit to use the
	/// default write concern.
	///
	/// When issued on a sharded cluster, mongos converts the write concern of the renameCollection
	/// command and its helper `db.collection.renameCollection()` to "majority".
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern:     Option<WriteConcern>
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
pub struct RenameCollectionOptions {
	pub drop_target: Option<bool>
}

/// The drop command removes an entire collection from a database.
///
/// https://docs.mongodb.com/manual/reference/command/drop/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DropCommand<'a> {
	/// The name of the collection to drop.
	pub drop:          &'a str,
	/// Optional. A document expressing the write concern of the drop command. Omit to use the
	/// default write concern.
	///
	/// When issued on a sharded cluster, mongos converts the write concern of the drop command
	/// and its helper `db.collection.drop()` to "majority".
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern: Option<WriteConcern>
}

/// Builds one or more indexes on a collection.
///
/// https://docs.mongodb.com/manual/reference/command/createIndexes/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIndexesCommand<'a> {
	/// The collection for which to create indexes.
	pub create_indexes: &'a str,
	/// Specifies the indexes to create. Each document in the array specifies a separate index.
	pub indexes:        &'a [CreateIndexesCommandIndex<'a>],
	/// Optional. A document expressing the write concern. Omit to use the default write concern.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern:  Option<WriteConcern>
}

/// https://docs.mongodb.com/manual/reference/command/createIndexes/
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIndexesCommandIndex<'a> {
	/// Specifies the index’s fields. For each field, specify a key-value pair in which the key is
	/// the name of the field to index and the value is either the index direction or index type.
	/// If specifying direction, specify 1 for ascending or -1 for descending.
	pub key:                       &'a [(&'a str, &'a str)],
	/// A name that uniquely identifies the index.
	pub name:                      Option<&'a str>,
	/// Optional. Creates a unique index so that the collection will not accept insertion or
	/// update of documents where the index key value matches an existing value in the index.
	///
	/// Specify true to create a unique index. The default value is false.
	///
	/// The option is unavailable for hashed indexes.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub unique:                    Option<bool>,
	/// Optional. If specified, the index only references documents that match the filter
	/// expression. See Partial Indexes for more information.
	///
	/// A filter expression can include:
	///
	/// - equality expressions (i.e. field: value or using the $eq operator),
	/// - `$exists`: true expression,
	/// - `$gt`, `$gte`, `$lt`, `$lte` expressions,
	/// - `$type` expressions,
	/// - `$and` operator at the top-level only
	///
	/// You can specify a partialFilterExpression option for all MongoDB index types.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub partial_filter_expression: Option<bson::Document>,
	/// Optional. If true, the index only references documents with the specified field. These
	/// indexes use less space but behave differently in some situations (particularly sorts).
	/// The default value is false. See Sparse Indexes for more information.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub sparse:                    Option<bool>,
	/// Optional. Specifies a value, in seconds, as a TTL to control how long MongoDB retains
	/// documents in this collection. See Expire Data from Collections by Setting TTL for more
	/// information on this functionality. This applies only to TTL indexes.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub expire_after_seconds:      Option<i32>,
	/// Optional. Allows users to configure the storage engine on a per-index basis when creating
	/// an index.
	///
	/// The storageEngine option should take the following form:
	///
	/// `storageEngine: { <storage-engine-name>: <options> }`
	///
	/// Storage engine configuration options specified when creating indexes are validated and
	/// logged to the oplog during replication to support replica sets with members that use
	/// different storage engines.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub storage_engine:            Option<bson::Document>,
	/// Optional. For text indexes, a document that contains field and weight pairs. The weight is
	/// an integer ranging from 1 to 99,999 and denotes the significance of the field relative to
	/// the other indexed fields in terms of the score. You can specify weights for some or all the
	/// indexed fields. See Control Search Results with Weights to adjust the scores. The default
	/// value is 1.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub weights:                   Option<bson::Document>,
	/// Optional. For text indexes, the language that determines the list of stop words and the
	/// rules for the stemmer and tokenizer. See Text Search Languages for the available languages
	/// and Specify a Language for Text Index for more information and examples. The default value
	/// is english.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub default_language:          Option<&'a str>,
	/// Optional. For text indexes, the name of the field, in the collection’s documents, that
	/// contains the override language for the document. The default value is language. See Use
	/// any Field to Specify the Language for a Document for an example.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub language_override:         Option<&'a str>,
	/// Optional. The text index version number. Users can use this option to override the default
	/// version number.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub text_index_version:        Option<i32>,
	/// Optional. The 2dsphere index version number. Users can use this option to override the
	/// default version number.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub _2d_sphere_index_version:  Option<i32>,
	/// Optional. For 2d indexes, the number of precision of the stored geohash value of the
	/// location data.
	///
	/// The bits value ranges from 1 to 32 inclusive. The default value is 26.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub bits:                      Option<u32>,
	/// Optional. For 2d indexes, the lower inclusive boundary for the longitude and latitude
	/// values. The default value is -180.0.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub min:                       Option<f64>,
	/// Optional. For 2d indexes, the upper inclusive boundary for the longitude and latitude
	/// values. The default value is 180.0.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max:                       Option<f64>,
	/// For geoHaystack indexes, specify the number of units within which to group the location
	/// values; i.e. group in the same bucket those location values that are within the specified
	/// number of units to each other.
	///
	/// The value must be greater than 0.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub bucket_size:               Option<i32>,
	/// Optional. Specifies the collation for the index.
	///
	/// Collation allows users to specify language-specific rules for string comparison, such as
	/// rules for lettercase and accent marks.
	///
	/// If you have specified a collation at the collection level, then:
	///
	/// - If you do not specify a collation when creating the index, MongoDB creates the index with
	/// the collection’s default collation.
	/// - If you do specify a collation when creating the index, MongoDB creates the index with the
	/// specified collation.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub collation:                 Option<Collation>,
	/// Optional. Allows users to include or exclude specific field paths from a wildcard index
	/// using the `{ "$**" : 1}` key pattern. This option is only valid if creating a wildcard
	/// index on all document fields. You cannot specify this option if creating a wildcard index
	/// on a specific field path and its subfields, e.g. `{ "path.to.field.$**" : 1 }`
	#[serde(skip_serializing_if = "Option::is_none")]
	pub wildcard_projection:       Option<bson::Document>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateIndexesCommandResult {
	pub created_collection_atomatically: Option<bool>,
	pub num_indexes_before:              Option<u32>,
	pub num_indexes_after:               Option<u32>,
	pub msg:                             Option<String>
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListIndexesCommand<'a> {
	pub list_indexes: &'a str
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DropIndexesCommand<'a, T: Serialize> {
	pub drop_indexes:  &'a str,
	pub index:         T,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub write_concern: Option<WriteConcern>
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteCommandResponse {
	pub n:                  i32,
	pub write_errors:       Option<Vec<CommandWriteError>>,
	pub write_conern_error: Option<Vec<CommandWriteConcernError>>,
	pub n_modified:         Option<i32>,
	pub upserted:           Option<Vec<Upserted>>
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandWriteError {
	pub code:     i32,
	pub err_info: Option<bson::Document>,
	pub errmsg:   String,
	pub index:    i32
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandWriteConcernError {
	pub code:     i32,
	pub err_info: Option<bson::Document>,
	pub errmsg:   String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Upserted {
	pub index: i32,
	pub _id:   ObjectId
}

impl WriteCommandResponse {
	fn into_result(mut self) -> Result<Self> {
		if let Some(e) = self.write_errors.as_mut().and_then(Vec::pop) {
			Err(Error::Operation(e.code.into(), e.errmsg))
		} else if let Some(e) = self.write_conern_error.as_mut().and_then(Vec::pop) {
			Err(Error::Operation(e.code.into(), e.errmsg))
		} else {
			Ok(self)
		}
	}
}