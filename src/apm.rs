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

use crate::{Client, bson::Document, Error};

pub type EventListener = Box<dyn Fn(&Client, &Event) + Send + Sync>;

pub type Address<'a> = (&'a str, u16);

#[derive(Debug)]
pub enum Event<'a> {
	CommandStarted {
		command:       &'a Document,
		database_name: Option<&'a str>,
		command_name:  Command,
		request_id:    usize,
		operation_id:  usize,
		connection_id: usize
	},
	CommandSucceeded {
		duration:      std::time::Duration,
		reply:         &'a Document,
		command_name:  Command,
		request_id:    usize,
		operation_id:  usize,
		connection_id: usize
	},
	CommandFailed {
		duration:      std::time::Duration,
		failure:       &'a Error,
		command_name:  Command,
		request_id:    usize,
		operation_id:  usize,
		connection_id: usize
	},
	PoolCreated {
		address: Address<'a>,
		options: ()
	},
	PoolCleared {
		address: Address<'a>
	},
	PoolClosed {
		address: Address<'a>
	},
	ConnectionCreated {
		address:       Address<'a>,
		connection_id: usize
	},
	ConnectionReady {
		address:       Address<'a>,
		connection_id: usize
	},
	ConnectionClosed {
		address:       Address<'a>,
		connection_id: usize,
		reason:        ConnectionClosedReason,
	},
	ConnectionCheckOutStarted {
		address:       Address<'a>,
	},
	ConnectionCheckOutFailed {
		address:       Address<'a>,
		reason:        ConnectionCheckOutFailedReason
	},
	ConnectionCheckedOut {
		address:       Address<'a>,
		connection_id: usize
	},
	ConnectionCheckedIn {
		address:       Address<'a>,
		connection_id: usize
	},
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Command {
	Aggregate,
	Count,
	Distinct,
	GeoSearch,
	Delete,
	Find,
	FindAndModify,
	GetMore,
	Insert,
	Update,
	Authenticate,
	Logout,
	IsMaster,
	Create,
	CreateIndexes,
	Drop,
	DropDatabase,
	DropIndexes,
	ListCollections,
	ListDatabases,
	ListIndexes,
	RenameCollection,
	Shutdown,
	Unknown
}

impl From<&str> for Command {
	fn from(s: &str) -> Self {
		use self::Command::*;
		match s {
			"aggregate"        => Aggregate,
			"count"            => Count,
			"distinct"         => Distinct,
			"geoSearch"        => GeoSearch,
			"delete"           => Delete,
			"find"             => Find,
			"findAndModify"    => FindAndModify,
			"getMore"          => GetMore,
			"insert"           => Insert,
			"update"           => Update,
			"authenticate"     => Authenticate,
			"logout"           => Logout,
			"isMaster"         => IsMaster,
			"create"           => Create,
			"createIndexes"    => CreateIndexes,
			"drop"             => Drop,
			"dropDatabase"     => DropDatabase,
			"dropIndexes"      => DropIndexes,
			"listCollections"  => ListCollections,
			"listDatabases"    => ListDatabases,
			"listIndexes"      => ListIndexes,
			"renameCollection" => RenameCollection,
			"shutdown"         => Shutdown,
			_                  => Unknown
		}
	}
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ConnectionCheckOutFailedReason {
	PoolClosed,
	Timeout,
	Error
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ConnectionClosedReason {
	Stale,
	Idle,
	Error,
	PoolClosed
}

