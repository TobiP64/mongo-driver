# mongo-driver
A high performance mongodb driver written in Rust.

This driver is not an official MongoDB product.

## Compatibility

The driver supports MongoDB 4.0 or newer and is to a large part compliant to the MongoDB Specs.

## Installation

Add this repository to your project's Cargo.toml:

```toml
mongo-driver = { git = "https://gitlab.com/TobiP64/mongo-driver.git" }
```

### Features

- tls (default): enable this feature to securely connect to your database server via tls, uses rustls
- auth (default): enable authentication
- compress (default): enable wire compression

## Usage

```rust
use mongo_driver::*;

fn main() {
    // connect to a server
    let client = ClientOptions {
        hosts: vec!["localhost:27017".to_String()]
        .. ClientOptions::default()
    }.connect()
            .expect("failed to init client");
    
    // OR

    // connect to a server with a connection string
    let client = ClientOptions::from_str("mongodb://localhost:27017/?serverSelectionTimeoutMS=500")
            .expect("invalid client options")
            .connect()
            .expect("failed to init client");

    // access a collection
    let coll = client.db("test").collection("test");

    // insert a document
    coll.insert_one(doc! { "test" => "test" }, None)
        .expect("failed to insert document");

    // find multiple documents
    let cursor = coll.find(None, None)
        .expect("failed to execute find");

    for doc in cursor {
        println!("{:?}", doc.expect("invalid document"));
    }
}
```