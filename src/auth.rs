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
	crate::{*, wire::Wire},
	serde::{Serialize, Deserialize},
	hmac::{Hmac, Mac, digest::generic_array::GenericArray},
	rand::Rng
};

#[derive(Debug, Serialize)]
struct AuthenticateCommand<'a> {
	authenticate: u32,
	mechanism:    AuthMech,
	user:         Option<&'a str>
}

#[derive(Debug, Deserialize)]
struct AuthenticateResponse<'a> {
	#[serde(rename = "$db")]
	db:   &'a str,
	user: &'a str
}

#[derive(Debug, Serialize)]
struct SaslStartCommand<'a> {
	sasl_start:     u32,
	mechanism:      AuthMech,
	payload:        &'a [u8],
	auto_authorize: u32
}

#[derive(Debug, Deserialize)]
struct SaslResponse<'a> {
	conversation_id: u32,
	code:            Option<u32>,
	done:            bool,
	payload:         &'a [u8]
}

#[derive(Debug, Serialize)]
struct SaslContinueCommand<'a> {
	sasl_continue:   u32,
	conversation_id: u32,
	payload:         &'a [u8]
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum AuthError {
	InvalidReply,
	InvalidServerNonce,
	InvalidServerSignature,
	InvalidIterationCount,
	ServerError,
	UnsupportedMechanism,
	InvalidOptions
}

pub(crate) fn auth(
	stream:     &mut impl Wire,
	request_id: i32,
	credential: &common::Credential
) -> Result<()> {
	match credential.mechanism.ok_or(Error::Auth(AuthError::InvalidOptions))? {
		AuthMech::MongoDbX509 => auth_x509(
			stream,
			request_id,
			credential.username.as_deref()
		),
		AuthMech::Plain => auth_plain(
			stream,
			request_id,
			credential.username.as_deref().ok_or(Error::Auth(AuthError::InvalidOptions))?,
			credential.password.as_deref().ok_or(Error::Auth(AuthError::InvalidOptions))?
		),
		AuthMech::ScramSha256 => auth_scram(
			stream,
			request_id,
			credential.username.as_deref().ok_or(Error::Auth(AuthError::InvalidOptions))?,
			credential.password.as_deref().ok_or(Error::Auth(AuthError::InvalidOptions))?
		),
		_ => Err(Error::Auth(AuthError::UnsupportedMechanism))
	}
}

fn auth_x509(
	stream:     &mut impl Wire,
	request_id: i32,
	username:   Option<&str>
) -> Result<()> {
	stream.send(request_id, None, &Document::from(&AuthenticateCommand {
		authenticate: 1,
		mechanism:    AuthMech::Plain,
		user:         username
	})?, &[])?;

	let doc = stream.recv(request_id)?;
	let _response = doc.deserialize::<AuthenticateResponse>()?;

	Ok(())
}

fn auth_plain(
	stream:     &mut impl Wire,
	request_id: i32,
	username:   &str,
	password:   &str
) -> Result<()> {
	stream.send(request_id, None,  &Document::from(&SaslStartCommand {
		sasl_start: 1,
		mechanism:  AuthMech::Plain,
		payload:    format!("\0{}\0{}", username, password).as_bytes(),
		auto_authorize: 0
	})?, &[])?;

	let doc = stream.recv(request_id)?;
	let response = doc.deserialize::<SaslResponse>()?;

	if !response.done {
		return Err(Error::Auth(AuthError::InvalidReply));
	}

	Ok(())
}

fn auth_scram(
	stream:     &mut impl Wire,
	request_id: i32,
	username:   &str,
	password:   &str
) -> Result<()> {

	// generate client nonce

	let mut client_nonce = [0u8; 32];
	for e in &mut client_nonce {
		*e = rand::thread_rng().gen_range(0x2Du8, 0x7Fu8);
	}

	// client first message

	let mut payload = Vec::with_capacity(8 + username.len());
	payload.extend_from_slice(b"n,,n=");
	payload.extend_from_slice(username.as_bytes());
	payload.extend_from_slice(b",r=");
	payload.extend_from_slice(&client_nonce);

	stream.send(request_id, None,  &Document::from(&SaslStartCommand {
		sasl_start: 1,
		mechanism:  AuthMech::ScramSha256,
		payload:    &payload,
		auto_authorize: 0
	})?, &[])?;

	// server first message

	let doc = stream.recv(request_id)?;
	let response: SaslResponse = doc.deserialize::<SaslResponse>()?;
	let sasl = std::str::from_utf8(response.payload).unwrap();

	let off = sasl.find("r=").ok_or(Error::Auth(AuthError::InvalidReply))?;
	let len = sasl[off..].find(',').unwrap_or(sasl.len() - off);
	let combined_nonce = sasl[off..off + len].as_bytes();

	let off = sasl.find("s=").ok_or(Error::Auth(AuthError::InvalidReply))?;
	let len = sasl[off..].find(',').unwrap_or(sasl.len() - off);
	let salt = base64::decode(&sasl[off..off + len])
		.map_err(|_| Error::Auth(AuthError::InvalidReply))?;

	let off = sasl.find("i=").ok_or(Error::Auth(AuthError::InvalidReply))?;
	let len = sasl[off..].find(',').unwrap_or(sasl.len() - off);
	let iterations = sasl[off..off + len].parse::<usize>()
		.map_err(|_| Error::Auth(AuthError::InvalidReply))?;

	if iterations < 4096 {
		return Err(Error::Auth(AuthError::InvalidIterationCount));
	} else if !combined_nonce.starts_with(&client_nonce) {
		return Err(Error::Auth(AuthError::InvalidServerNonce));
	}

	// client final message

	payload.push(b',');
	payload.extend_from_slice(response.payload);
	payload.push(b',');
	payload.extend_from_slice(b"c=biws,r=");
	payload.extend_from_slice(combined_nonce);

	let (client_proof, server_signature) = compute_client_proof(
		username,
		password,
		&salt,
		iterations,
		&payload[3..]
	);

	payload.clear();
	payload.extend_from_slice(b"c=biws,r=");
	payload.extend_from_slice(combined_nonce);
	payload.extend_from_slice(b",p=");
	payload.extend_from_slice(base64::encode(&client_proof).as_bytes());

	stream.send(request_id, None, &Document::from(&SaslContinueCommand {
		sasl_continue:   1,
		payload:         &payload,
		conversation_id: response.conversation_id
	})?, &[])?;

	// server final message

	let doc = stream.recv(request_id)?;
	let response = doc.deserialize::<SaslResponse>()?;
	let sasl = std::str::from_utf8(response.payload).unwrap();

	if !response.done {
		return Err(Error::Auth(AuthError::InvalidReply));
	}

	let off = sasl.find("s=").ok_or(Error::Auth(AuthError::InvalidReply))?;
	let len = sasl[off..].find(',').unwrap_or(sasl.len() - off);
	let verifier = base64::decode(&sasl[off..off + len])
		.map_err(|_| Error::Auth(AuthError::InvalidReply))?;

	if verifier != server_signature {
		return Err(Error::Auth(AuthError::InvalidServerSignature))
	}

	Ok(())
}

fn compute_client_proof(
	username: &str,
	password: &str,
	salt:     &[u8],
	i:        usize,
	auth_msg: &[u8]
) -> ([u8; 32], [u8; 32]) {

	// salted password

	// TODO saslPrep
	let mut pwd = [0u8; 32];
	crate::common::bytes_to_hex(&*md5::compute(
		format!("{}:mongo:{}", username, password)), &mut pwd);
	let mut salted_pwd = [0u8; 32];
	pbkdf2::pbkdf2::<hmac::Hmac<sha2::Sha256>>(
		&pwd, salt, i, &mut salted_pwd);

	// client/server key

	let mut hmac = Hmac::<sha2::Sha256>::new(
		GenericArray::from_slice(&salted_pwd));
	hmac.input(b"Client Key");
	let client_key: [u8; 32] = hmac.result().code().into();

	let mut hmac = Hmac::<sha2::Sha256>::new(
		GenericArray::from_slice(&salted_pwd));
	hmac.input(b"Server Key");
	let server_key: [u8; 32] = hmac.result().code().into();

	// stored key

	let stored_key: [u8; 32] = Hmac::<sha2::Sha256>::new_varkey(&client_key)
		.unwrap().result().code().into();

	// client/server signature

	let mut hmac = Hmac::<sha2::Sha256>::new(
		GenericArray::from_slice(&stored_key));
	hmac.input(auth_msg);
	let client_signature: [u8; 32] = hmac.result().code().into();

	let mut hmac = Hmac::<sha2::Sha256>::new(
		GenericArray::from_slice(&server_key));
	hmac.input(auth_msg);
	let server_signature: [u8; 32] = hmac.result().code().into();

	// client proof

	let mut client_proof = [0u8; 32];
	for i in 0..32 {
		client_proof[i] = client_key[i] ^ client_signature[i];
	}

	(client_proof, server_signature)
}