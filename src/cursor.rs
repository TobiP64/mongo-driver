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
	crate::{Result, ReadPreference, DataBase, bson::Document},
	std::collections::VecDeque,
	serde::{Serialize, Deserialize}
};

#[derive(Clone, Debug)]
pub struct Cursor {
	pub db:                      DataBase,
	pub cursor_id:               i64,
	pub namespace:               String,
	pub post_batch_resume_token: Option<Document>,
	pub batch_size:              Option<i64>,
	pub max_time_ms:             Option<i64>,
	pub buffer:                  VecDeque<Document>,
	pub read_preference:         ReadPreference
}

impl Iterator for Cursor {
	type Item = Result<Document>;
	
	fn next(&mut self) -> Option<Self::Item> {
		if let Some(e) = self.buffer.pop_front() {
			return Some(Ok(e))
		} else if self.cursor_id == 0 {
			return None;
		}
		
		let result: GetMoreCommandResult = match self.db.run_command(&GetMoreCommand {
			get_more:    self.cursor_id,
			collection:  &self.namespace[self.namespace.find('.').unwrap() + 1..],
			batch_size:  self.batch_size,
			max_time_ms: self.max_time_ms
		}, Some(&self.read_preference), false) {
			Ok(v) => v,
			Err(e) => return Some(Err(e))
		};
		
		self.buffer.extend(result.cursor.batch.into_iter());
		self.cursor_id = result.cursor.id;
		self.namespace = result.cursor.ns;
		self.post_batch_resume_token = result.cursor.post_batch_resume_token;
		self.next()
	}
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GetMoreCommand<'a> {
	get_more:    i64,
	collection:  &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	batch_size:  Option<i64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	max_time_ms: Option<i64>
}

#[derive(Debug, Deserialize)]
pub(super) struct GetMoreCommandResult {
	pub(super) cursor: GetMoreCommandResultCursor
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetMoreCommandResultCursor {
	#[serde(alias = "firstBatch", alias = "nextBatch")]
	pub(super) batch:                   Vec<Document>,
	pub(super) id:                      i64,
	pub(super) ns:                      String,
	pub(super) post_batch_resume_token: Option<Document>
}

#[derive(Clone, Debug)]
pub struct ChangeStream(pub(super) Cursor);

impl Iterator for ChangeStream {
	type Item = <Cursor as Iterator>::Item;

	fn next(&mut self) -> Option<Self::Item> {
		self.0.next()
	}
}

impl std::ops::Deref for ChangeStream {
	type Target = Cursor;
	
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl std::ops::DerefMut for ChangeStream {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}