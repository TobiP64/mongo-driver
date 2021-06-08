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

use {crate::*};

pub trait StageTrait: Into<Vec<Document>> {
	fn set(self, doc: Document) -> Stage<Self, Set> {
		Stage(self, Set { set: doc })
	}
	
	fn count(self, count: &str) -> Stage<Self, Count> {
		Stage(self, Count { count })
	}
	
	fn group(self, group: Document) -> Stage<Self, Group> {
		Stage(self, Group { group })
	}
	
	fn lookup(self, lookup: LookupInner) -> Stage<Self, Lookup> {
		Stage(self, Lookup { lookup })
	}
	
	fn r#match(self, r#match: Document) -> Stage<Self, Match> {
		Stage(self, Match { r#match })
	}
	
	fn project(self, project: Document) -> Stage<Self, Project> {
		Stage(self, Project { project })
	}
	
	fn replace_with<'a, T: Into<Bson<'a>>>(self, replace_with: T) -> Stage<Self, ReplaceWith<'a>> {
		Stage(self, ReplaceWith { replace_with: replace_with.into() })
	}
	
	fn sort(self, sort: Document) -> Stage<Self, Sort> {
		Stage(self, Sort { sort })
	}
	
	fn unwind(self, unwind: &str) -> Stage<Self, Unwind> {
		Stage(self, Unwind { unwind })
	}
	
	fn finish(self) -> Vec<Document> {
		self.into()
	}
	
	fn aggregate(self, collection: &Collection, options:  Option<AggregateOptions>) -> Result<Cursor> {
		collection.aggregate(self.into().as_slice(), options)
	}
	
	fn update_one(
		self,
		collection: &Collection,
		filter:     bson::Document,
		options:    Option<UpdateOptions>
	) -> Result<UpdateResult> {
		collection.update_one_pipeline(filter, self.into().as_slice(), options)
	}
}

impl StageTrait for Pipeline {}
impl<T: Into<Vec<Document>>, U: serde::Serialize> StageTrait for Stage<T, U> {}

pub struct Pipeline;

impl Pipeline {
	pub fn build() -> Self {
		Self
	}
}

impl Into<Vec<Document>> for Pipeline {
	fn into(self) -> Vec<Document> {
		Vec::new()
	}
}

pub struct Stage<T: Into<Vec<Document>>, U: serde::Serialize>(T, U);

impl<T: Into<Vec<Document>>, U: serde::Serialize> Into<Vec<Document>> for Stage<T, U> {
	fn into(self) -> Vec<Document> {
		let mut vec = self.0.into();
		vec.push(Document::from(&self.1).unwrap());
		vec
	}
}

macro_rules! stage {
    ($name:ident, $field:ident, $serde:expr, $ty:ty) => {
    	#[derive(Serialize)]
		pub struct $name {
			#[serde(rename = $serde)]
			$field: $ty
		}
    };
    (@lifetime $name:ident, $field:ident, $serde:expr, $ty:ty) => {
    	#[derive(Serialize)]
		pub struct $name<'a> {
			#[serde(rename = $serde)]
			$field: $ty
		}
    };
}

stage!(Set, set, "$set", Document);
stage!(Bucket, bucket, "$bucket", BucketInner);
stage!(@lifetime Count, count, "$count", &'a str);
stage!(Group, group, "$group", Document);
stage!(@lifetime Lookup, lookup, "$lookup", LookupInner<'a>);
stage!(Match, r#match, "$match", Document);
stage!(Project, project, "$project", Document);
stage!(@lifetime ReplaceWith, replace_with, "$replaceWith", Bson<'a>);
stage!(Sort, sort, "$sort", Document);
stage!(@lifetime Unwind, unwind, "$unwind", &'a str);

#[derive(Serialize)]
pub struct BucketInner {

}

#[derive(Serialize)]
pub struct LookupInner<'a> {
	pub from:          &'a str,
	#[serde(rename = "localField")]
	pub local_field:   &'a str,
	#[serde(rename = "foreignField")]
	pub foreign_field: &'a str,
	#[serde(rename = "as")]
	pub r#as:          &'a str
}

/// A wrapper that implements `Debug` for a type that doesn't.
#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Default)]
pub struct __DebugWrapper__<T>(pub T);

impl<T> std::fmt::Debug for __DebugWrapper__<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("NonDebugStruct")
			.finish()
	}
}

impl<T> std::ops::Deref for __DebugWrapper__<T> {
	type Target = T;
	
	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl<T> std::ops::DerefMut for __DebugWrapper__<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}
