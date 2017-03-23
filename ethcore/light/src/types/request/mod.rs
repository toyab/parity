// Copyright 2015-2017 Parity Technologies (UK) Ltd.
// This file is part of Parity.

// Parity is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity.  If not, see <http://www.gnu.org/licenses/>.

//! Light protocol request types.

use rlp::{Encodable, Decodable, DecoderError, RlpStream, UntrustedRlp};
use util::H256;

mod builder;

// re-exports of request types.
pub use self::header::{
	Complete as CompleteHeadersRequest,
	Incomplete as IncompleteHeadersRequest,
	Response as HeadersResponse
};
pub use self::header_proof::{
	Complete as CompleteHeaderProofRequest,
	Incomplete as IncompleteHeaderProofRequest,
	Response as HeaderProofResponse
};
pub use self::block_body::{
	Complete as CompleteBodyRequest,
	Incomplete as IncompleteBodyRequest,
	Response as BodyResponse
};
pub use self::block_receipts::{
	Complete as CompleteReceiptsRequest,
	Incomplete as IncompleteReceiptsRequest,
	Response as ReceiptsResponse
};
pub use self::account::{
	Complete as CompleteAccountRequest,
	Incomplete as IncompleteAccountRequest,
	Response as AccountResponse,
};
pub use self::storage::{
	Complete as CompleteStorageRequest,
	Incomplete as IncompleteStorageRequest,
	Response as StorageResponse
};
pub use self::contract_code::{
	Complete as CompleteCodeRequest,
	Incomplete as IncompleteCodeRequest,
	Response as CodeResponse,
};
pub use self::execution::{
	Complete as CompleteExecutionRequest,
	Incomplete as IncompleteExecutionRequest,
	Response as ExecutionResponse,
};

pub use self::builder::{RequestBuilder, Requests};

/// Error indicating a reference to a non-existent or wrongly-typed output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoSuchOutput;

/// Error on processing a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseError {
	/// Wrong kind of response.
	WrongKind,
	/// No responses expected.
	Unexpected,
}

/// An input to a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field<T> {
	/// A pre-specified input.
	Scalar(T),
	/// An input which can be resolved later on.
	/// (Request index, output index)
	BackReference(usize, usize),
}

impl<T> Field<T> {
	// attempt conversion into scalar value.
	fn into_scalar(self) -> Result<T, NoSuchOutput> {
		match self {
			Field::Scalar(val) => Ok(val),
			_ => Err(NoSuchOutput),
		}
	}
}

impl<T> From<T> for Field<T> {
	fn from(val: T) -> Self {
		Field::Scalar(val)
	}
}

impl<T: Decodable> Decodable for Field<T> {
	fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
		match rlp.val_at::<u8>(0)? {
			0 => Ok(Field::Scalar(rlp.val_at::<T>(1)?)),
			1 => Ok({
				let inner_rlp = rlp.at(1)?;
				Field::BackReference(inner_rlp.val_at(0)?, inner_rlp.val_at(1)?)
			}),
			_ => Err(DecoderError::Custom("Unknown discriminant for PIP field.")),
		}
	}
}

impl<T: Encodable> Encodable for Field<T> {
	fn rlp_append(&self, s: &mut RlpStream) {
		s.begin_list(2);
		match *self {
			Field::Scalar(ref data) => {
				s.append(&0u8).append(data);
			}
			Field::BackReference(ref req, ref idx) => {
				s.append(&1u8).begin_list(2).append(req).append(idx);
			}
		}
	}
}

/// Request outputs which can be reused as inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
	/// A 32-byte hash output.
	Hash(H256),
	/// An unsigned-integer output.
	Number(u64),
}

impl Output {
	/// Get the output kind.
	pub fn kind(&self) -> OutputKind {
		match *self {
			Output::Hash(_) => OutputKind::Hash,
			Output::Number(_) => OutputKind::Number,
		}
	}
}

/// Response output kinds which can be used as back-references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputKind {
	/// A 32-byte hash output.
	Hash,
	/// An unsigned-integer output.
	Number,
}

/// Either a hash or a number.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "ipc", binary)]
pub enum HashOrNumber {
	/// Block hash variant.
	Hash(H256),
	/// Block number variant.
	Number(u64),
}

impl From<H256> for HashOrNumber {
	fn from(hash: H256) -> Self {
		HashOrNumber::Hash(hash)
	}
}

impl From<u64> for HashOrNumber {
	fn from(num: u64) -> Self {
		HashOrNumber::Number(num)
	}
}

impl Decodable for HashOrNumber {
	fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
		rlp.as_val::<H256>().map(HashOrNumber::Hash)
			.or_else(|_| rlp.as_val().map(HashOrNumber::Number))
	}
}

impl Encodable for HashOrNumber {
	fn rlp_append(&self, s: &mut RlpStream) {
		match *self {
			HashOrNumber::Hash(ref hash) => s.append(hash),
			HashOrNumber::Number(ref num) => s.append(num),
		};
	}
}

/// All request types, as they're sent over the network.
/// They may be incomplete, with back-references to outputs
/// of prior requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
	/// A request for block headers.
	Headers(IncompleteHeadersRequest),
	/// A request for a header proof (from a CHT)
	HeaderProof(IncompleteHeaderProofRequest),
	// TransactionIndex,
	/// A request for a block's receipts.
	Receipts(IncompleteReceiptsRequest),
	/// A request for a block body.
	Body(IncompleteBodyRequest),
	/// A request for a merkle proof of an account.
	Account(IncompleteAccountRequest),
	/// A request for a merkle proof of contract storage.
	Storage(IncompleteStorageRequest),
	/// A request for contract code.
	Code(IncompleteCodeRequest),
	/// A request for proof of execution,
	Execution(IncompleteExecutionRequest),
}

/// All request types, in an answerable state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteRequest {
	/// A request for block headers.
	Headers(CompleteHeadersRequest),
	/// A request for a header proof (from a CHT)
	HeaderProof(CompleteHeaderProofRequest),
	// TransactionIndex,
	/// A request for a block's receipts.
	Receipts(CompleteReceiptsRequest),
	/// A request for a block body.
	Body(CompleteBodyRequest),
	/// A request for a merkle proof of an account.
	Account(CompleteAccountRequest),
	/// A request for a merkle proof of contract storage.
	Storage(CompleteStorageRequest),
	/// A request for contract code.
	Code(CompleteCodeRequest),
	/// A request for proof of execution,
	Execution(CompleteExecutionRequest),
}

impl Request {
	fn kind(&self) -> Kind {
		match *self {
			Request::Headers(_) => Kind::Headers,
			Request::HeaderProof(_) => Kind::HeaderProof,
			Request::Receipts(_) => Kind::Receipts,
			Request::Body(_) => Kind::Body,
			Request::Account(_) => Kind::Account,
			Request::Storage(_) => Kind::Storage,
			Request::Code(_) => Kind::Code,
			Request::Execution(_) => Kind::Execution,
		}
	}
}

impl Decodable for Request {
	fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
		match rlp.val_at::<Kind>(0)? {
			Kind::Headers => Ok(Request::Headers(rlp.val_at(1)?)),
			Kind::HeaderProof => Ok(Request::HeaderProof(rlp.val_at(1)?)),
			Kind::Receipts => Ok(Request::Receipts(rlp.val_at(1)?)),
			Kind::Body => Ok(Request::Body(rlp.val_at(1)?)),
			Kind::Account => Ok(Request::Account(rlp.val_at(1)?)),
			Kind::Storage => Ok(Request::Storage(rlp.val_at(1)?)),
			Kind::Code => Ok(Request::Code(rlp.val_at(1)?)),
			Kind::Execution => Ok(Request::Execution(rlp.val_at(1)?)),
		}
	}
}

impl Encodable for Request {
	fn rlp_append(&self, s: &mut RlpStream) {
		s.begin_list(2);

		// hack around https://github.com/ethcore/parity/issues/4356
		Encodable::rlp_append(&self.kind(), s);

		match *self {
			Request::Headers(ref req) => s.append(req),
			Request::HeaderProof(ref req) => s.append(req),
			Request::Receipts(ref req) => s.append(req),
			Request::Body(ref req) => s.append(req),
			Request::Account(ref req) => s.append(req),
			Request::Storage(ref req) => s.append(req),
			Request::Code(ref req) => s.append(req),
			Request::Execution(ref req) => s.append(req),
		};
	}
}

impl IncompleteRequest for Request {
	type Complete = CompleteRequest;

	fn check_outputs<F>(&self, f: F) -> Result<(), NoSuchOutput>
		where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>
	{
		match *self {
			Request::Headers(ref req) => req.check_outputs(f),
			Request::HeaderProof(ref req) => req.check_outputs(f),
			Request::Receipts(ref req) => req.check_outputs(f),
			Request::Body(ref req) => req.check_outputs(f),
			Request::Account(ref req) => req.check_outputs(f),
			Request::Storage(ref req) => req.check_outputs(f),
			Request::Code(ref req) => req.check_outputs(f),
			Request::Execution(ref req) => req.check_outputs(f),
		}
	}

	fn note_outputs<F>(&self, f: F) where F: FnMut(usize, OutputKind) {
		match *self {
			Request::Headers(ref req) => req.note_outputs(f),
			Request::HeaderProof(ref req) => req.note_outputs(f),
			Request::Receipts(ref req) => req.note_outputs(f),
			Request::Body(ref req) => req.note_outputs(f),
			Request::Account(ref req) => req.note_outputs(f),
			Request::Storage(ref req) => req.note_outputs(f),
			Request::Code(ref req) => req.note_outputs(f),
			Request::Execution(ref req) => req.note_outputs(f),
		}
	}

	fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput> {
		match *self {
			Request::Headers(ref mut req) => req.fill(oracle),
			Request::HeaderProof(ref mut req) => req.fill(oracle),
			Request::Receipts(ref mut req) => req.fill(oracle),
			Request::Body(ref mut req) => req.fill(oracle),
			Request::Account(ref mut req) => req.fill(oracle),
			Request::Storage(ref mut req) => req.fill(oracle),
			Request::Code(ref mut req) => req.fill(oracle),
			Request::Execution(ref mut req) => req.fill(oracle),
		}
	}

	fn complete(self) -> Result<Self::Complete, NoSuchOutput> {
		match self {
			Request::Headers(req) => req.complete().map(CompleteRequest::Headers),
			Request::HeaderProof(req) => req.complete().map(CompleteRequest::HeaderProof),
			Request::Receipts(req) => req.complete().map(CompleteRequest::Receipts),
			Request::Body(req) => req.complete().map(CompleteRequest::Body),
			Request::Account(req) => req.complete().map(CompleteRequest::Account),
			Request::Storage(req) => req.complete().map(CompleteRequest::Storage),
			Request::Code(req) => req.complete().map(CompleteRequest::Code),
			Request::Execution(req) => req.complete().map(CompleteRequest::Execution),
		}
	}
}

/// Kinds of requests.
/// Doubles as the "ID" field of the request.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
	/// A request for headers.
	Headers = 0,
	/// A request for a header proof.
	HeaderProof = 1,
	// TransactionIndex = 2,
	/// A request for block receipts.
	Receipts = 3,
	/// A request for a block body.
	Body = 4,
	/// A request for an account + merkle proof.
	Account = 5,
	/// A request for contract storage + merkle proof
	Storage = 6,
	/// A request for contract.
	Code = 7,
	/// A request for transaction execution + state proof.
	Execution = 8,
}

impl Decodable for Kind {
	fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
		match rlp.as_val::<u8>()? {
			0 => Ok(Kind::Headers),
			1 => Ok(Kind::HeaderProof),
			// 2 => Ok(Kind::TransactionIndex),
			3 => Ok(Kind::Receipts),
			4 => Ok(Kind::Body),
			5 => Ok(Kind::Account),
			6 => Ok(Kind::Storage),
			7 => Ok(Kind::Code),
			8 => Ok(Kind::Execution),
			_ => Err(DecoderError::Custom("Unknown PIP request ID.")),
		}
	}
}

impl Encodable for Kind {
	fn rlp_append(&self, s: &mut RlpStream) {
		s.append(&(*self as u8));
	}
}

/// All response types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
	/// A response for block headers.
	Headers(HeadersResponse),
	/// A response for a header proof (from a CHT)
	HeaderProof(HeaderProofResponse),
	// TransactionIndex,
	/// A response for a block's receipts.
	Receipts(ReceiptsResponse),
	/// A response for a block body.
	Body(BodyResponse),
	/// A response for a merkle proof of an account.
	Account(AccountResponse),
	/// A response for a merkle proof of contract storage.
	Storage(StorageResponse),
	/// A response for contract code.
	Code(CodeResponse),
	/// A response for proof of execution,
	Execution(ExecutionResponse),
}

impl Response {
	/// Fill reusable outputs by writing them into the function.
	pub fn fill_outputs<F>(&self, f: F) where F: FnMut(usize, Output) {
		match *self {
			Response::Headers(ref res) => res.fill_outputs(f),
			Response::HeaderProof(ref res) => res.fill_outputs(f),
			Response::Receipts(ref res) => res.fill_outputs(f),
			Response::Body(ref res) => res.fill_outputs(f),
			Response::Account(ref res) => res.fill_outputs(f),
			Response::Storage(ref res) => res.fill_outputs(f),
			Response::Code(ref res) => res.fill_outputs(f),
			Response::Execution(ref res) => res.fill_outputs(f),
		}
	}

	fn kind(&self) -> Kind {
		match *self {
			Response::Headers(_) => Kind::Headers,
			Response::HeaderProof(_) => Kind::HeaderProof,
			Response::Receipts(_) => Kind::Receipts,
			Response::Body(_) => Kind::Body,
			Response::Account(_) => Kind::Account,
			Response::Storage(_) => Kind::Storage,
			Response::Code(_) => Kind::Code,
			Response::Execution(_) => Kind::Execution,
		}
	}
}

impl Decodable for Response {
	fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
		match rlp.val_at::<Kind>(0)? {
			Kind::Headers => Ok(Response::Headers(rlp.val_at(1)?)),
			Kind::HeaderProof => Ok(Response::HeaderProof(rlp.val_at(1)?)),
			Kind::Receipts => Ok(Response::Receipts(rlp.val_at(1)?)),
			Kind::Body => Ok(Response::Body(rlp.val_at(1)?)),
			Kind::Account => Ok(Response::Account(rlp.val_at(1)?)),
			Kind::Storage => Ok(Response::Storage(rlp.val_at(1)?)),
			Kind::Code => Ok(Response::Code(rlp.val_at(1)?)),
			Kind::Execution => Ok(Response::Execution(rlp.val_at(1)?)),
		}
	}
}

impl Encodable for Response {
	fn rlp_append(&self, s: &mut RlpStream) {
		s.begin_list(2);

		// hack around https://github.com/ethcore/parity/issues/4356
		Encodable::rlp_append(&self.kind(), s);

		match *self {
			Response::Headers(ref res) => s.append(res),
			Response::HeaderProof(ref res) => s.append(res),
			Response::Receipts(ref res) => s.append(res),
			Response::Body(ref res) => s.append(res),
			Response::Account(ref res) => s.append(res),
			Response::Storage(ref res) => s.append(res),
			Response::Code(ref res) => s.append(res),
			Response::Execution(ref res) => s.append(res),
		};
	}
}

/// A potentially incomplete request.
pub trait IncompleteRequest: Sized {
	/// The complete variant of this request.
	type Complete;

	/// Check prior outputs against the needed inputs.
	///
	/// This is called to ensure consistency of this request with
	/// others in the same packet.
	fn check_outputs<F>(&self, f: F) -> Result<(), NoSuchOutput>
		where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>;

	/// Note that this request will produce the following outputs.
	fn note_outputs<F>(&self, f: F) where F: FnMut(usize, OutputKind);

	/// Fill fields of the request.
	///
	/// This function is provided an "output oracle" which allows fetching of
	/// prior request outputs.
	/// Only outputs previously checked with `check_outputs` may be available.
	fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput>;

	/// Attempt to convert this request into its complete variant.
	/// Will succeed if all fields have been filled, will fail otherwise.
	fn complete(self) -> Result<Self::Complete, NoSuchOutput>;
}

/// Header request.
pub mod header {
	use super::{Field, HashOrNumber, NoSuchOutput, OutputKind, Output};
	use ethcore::encoded;
	use rlp::{Encodable, Decodable, DecoderError, RlpStream, UntrustedRlp};

	/// Potentially incomplete headers request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Incomplete {
		/// Start block.
		pub start: Field<HashOrNumber>,
		/// Skip between.
		pub skip: u64,
		/// Maximum to return.
		pub max: u64,
		/// Whether to reverse from start.
		pub reverse: bool,
	}

	impl Decodable for Incomplete {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Incomplete {
				start: rlp.val_at(0)?,
				skip: rlp.val_at(1)?,
				max: rlp.val_at(2)?,
				reverse: rlp.val_at(3)?
			})
		}
	}

	impl Encodable for Incomplete {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(4)
				.append(&self.start)
				.append(&self.skip)
				.append(&self.max)
				.append(&self.reverse);
		}
	}

	impl super::IncompleteRequest for Incomplete {
		type Complete = Complete;

		fn check_outputs<F>(&self, mut f: F) -> Result<(), NoSuchOutput>
			where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>
		{
			match self.start {
				Field::Scalar(_) => Ok(()),
				Field::BackReference(req, idx) =>
					f(req, idx, OutputKind::Hash).or_else(|_| f(req, idx, OutputKind::Number))
			}
		}

		fn note_outputs<F>(&self, _: F) where F: FnMut(usize, OutputKind) { }

		fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput> {
			if let Field::BackReference(req, idx) = self.start {
				self.start = match oracle(req, idx) {
					Ok(Output::Hash(hash)) => Field::Scalar(hash.into()),
					Ok(Output::Number(num)) => Field::Scalar(num.into()),
					Err(_) => Field::BackReference(req, idx),
				}
			}
		}

		fn complete(self) -> Result<Self::Complete, NoSuchOutput> {
			Ok(Complete {
				start: self.start.into_scalar()?,
				skip: self.skip,
				max: self.max,
				reverse: self.reverse,
			})
		}
	}

	/// A complete header request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Complete {
		/// Start block.
		pub start: HashOrNumber,
		/// Skip between.
		pub skip: u64,
		/// Maximum to return.
		pub max: u64,
		/// Whether to reverse from start.
		pub reverse: bool,
	}

	/// The output of a request for headers.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Response {
		/// The headers requested.
		pub headers: Vec<encoded::Header>,
	}

	impl Response {
		/// Fill reusable outputs by writing them into the function.
		pub fn fill_outputs<F>(&self, _: F) where F: FnMut(usize, Output) { }
	}

	impl Decodable for Response {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			use ethcore::header::Header as FullHeader;

			let mut headers = Vec::new();

			for item in rlp.iter() {
				// check that it's a valid encoding.
				// TODO: just return full headers here?
				let _: FullHeader = item.as_val()?;
				headers.push(encoded::Header::new(item.as_raw().to_owned()));
			}

			Ok(Response {
				headers: headers,
			})
		}
	}

	impl Encodable for Response {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(self.headers.len());
			for header in &self.headers {
				s.append_raw(header.rlp().as_raw(), 1);
			}
		}
	}
}

/// Request and response for header proofs.
pub mod header_proof {
	use super::{Field, NoSuchOutput, OutputKind, Output};
	use rlp::{Encodable, Decodable, DecoderError, RlpStream, UntrustedRlp};
	use util::{Bytes, U256, H256};

	/// Potentially incomplete header proof request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Incomplete {
		/// Block number.
		pub num: Field<u64>,
	}

	impl Decodable for Incomplete {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Incomplete {
				num: rlp.val_at(0)?,
			})
		}
	}

	impl Encodable for Incomplete {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(1).append(&self.num);
		}
	}

	impl super::IncompleteRequest for Incomplete {
		type Complete = Complete;

		fn check_outputs<F>(&self, mut f: F) -> Result<(), NoSuchOutput>
			where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>
		{
			match self.num {
				Field::Scalar(_) => Ok(()),
				Field::BackReference(req, idx) => f(req, idx, OutputKind::Number),
			}
		}

		fn note_outputs<F>(&self, mut note: F) where F: FnMut(usize, OutputKind) {
			note(0, OutputKind::Hash);
		}

		fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput> {
			if let Field::BackReference(req, idx) = self.num {
				self.num = match oracle(req, idx) {
					Ok(Output::Number(num)) => Field::Scalar(num.into()),
					_ => Field::BackReference(req, idx),
				}
			}
		}

		fn complete(self) -> Result<Self::Complete, NoSuchOutput> {
			Ok(Complete {
				num: self.num.into_scalar()?,
			})
		}
	}

	/// A complete header proof request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Complete {
		/// The number to get a header proof for.
		pub num: u64,
	}

	/// The output of a request for a header proof.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Response {
		/// Inclusion proof of the header and total difficulty in the CHT.
		pub proof: Vec<Bytes>,
		/// The proved header's hash.
		pub hash: H256,
		/// The proved header's total difficulty.
		pub td: U256,
	}

	impl Response {
		/// Fill reusable outputs by providing them to the function.
		pub fn fill_outputs<F>(&self, mut f: F) where F: FnMut(usize, Output) {
			f(0, Output::Hash(self.hash));
		}
	}

	impl Decodable for Response {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {

			Ok(Response {
				proof: rlp.list_at(0)?,
				hash: rlp.val_at(1)?,
				td: rlp.val_at(2)?,
			})
		}
	}

	impl Encodable for Response {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(3).begin_list(self.proof.len());
			for item in &self.proof {
				s.append_list(&item);
			}

			s.append(&self.hash).append(&self.td);
		}
	}
}

/// Request and response for block receipts
pub mod block_receipts {
	use super::{Field, NoSuchOutput, OutputKind, Output};
	use ethcore::receipt::Receipt;
	use rlp::{Encodable, Decodable, DecoderError, RlpStream, UntrustedRlp};
	use util::H256;

	/// Potentially incomplete block receipts request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Incomplete {
		/// Block hash to get receipts for.
		pub hash: Field<H256>,
	}

	impl Decodable for Incomplete {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Incomplete {
				hash: rlp.val_at(0)?,
			})
		}
	}

	impl Encodable for Incomplete {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(1).append(&self.hash);
		}
	}

	impl super::IncompleteRequest for Incomplete {
		type Complete = Complete;

		fn check_outputs<F>(&self, mut f: F) -> Result<(), NoSuchOutput>
			where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>
		{
			match self.hash {
				Field::Scalar(_) => Ok(()),
				Field::BackReference(req, idx) => f(req, idx, OutputKind::Hash),
			}
		}

		fn note_outputs<F>(&self, _: F) where F: FnMut(usize, OutputKind) {}

		fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput> {
			if let Field::BackReference(req, idx) = self.hash {
				self.hash = match oracle(req, idx) {
					Ok(Output::Number(hash)) => Field::Scalar(hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}
		}

		fn complete(self) -> Result<Self::Complete, NoSuchOutput> {
			Ok(Complete {
				hash: self.hash.into_scalar()?,
			})
		}
	}

	/// A complete block receipts request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Complete {
		/// The number to get block receipts for.
		pub hash: H256,
	}

	/// The output of a request for block receipts.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Response {
		/// The block receipts.
		pub receipts: Vec<Receipt>
	}

	impl Response {
		/// Fill reusable outputs by providing them to the function.
		pub fn fill_outputs<F>(&self, _: F) where F: FnMut(usize, Output) {}
	}

	impl Decodable for Response {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {

			Ok(Response {
				receipts: rlp.as_list()?,
			})
		}
	}

	impl Encodable for Response {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.append_list(&self.receipts);
		}
	}
}

/// Request and response for a block body
pub mod block_body {
	use super::{Field, NoSuchOutput, OutputKind, Output};
	use ethcore::encoded;
	use rlp::{Encodable, Decodable, DecoderError, RlpStream, UntrustedRlp};
	use util::H256;

	/// Potentially incomplete block body request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Incomplete {
		/// Block hash to get receipts for.
		pub hash: Field<H256>,
	}

	impl Decodable for Incomplete {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Incomplete {
				hash: rlp.val_at(0)?,
			})
		}
	}

	impl Encodable for Incomplete {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(1).append(&self.hash);
		}
	}

	impl super::IncompleteRequest for Incomplete {
		type Complete = Complete;

		fn check_outputs<F>(&self, mut f: F) -> Result<(), NoSuchOutput>
			where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>
		{
			match self.hash {
				Field::Scalar(_) => Ok(()),
				Field::BackReference(req, idx) => f(req, idx, OutputKind::Hash),
			}
		}

		fn note_outputs<F>(&self, _: F) where F: FnMut(usize, OutputKind) {}

		fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput> {
			if let Field::BackReference(req, idx) = self.hash {
				self.hash = match oracle(req, idx) {
					Ok(Output::Hash(hash)) => Field::Scalar(hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}
		}

		fn complete(self) -> Result<Self::Complete, NoSuchOutput> {
			Ok(Complete {
				hash: self.hash.into_scalar()?,
			})
		}
	}

	/// A complete block body request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Complete {
		/// The hash to get a block body for.
		pub hash: H256,
	}

	/// The output of a request for block body.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Response {
		/// The block body.
		pub body: encoded::Body,
	}

	impl Response {
		/// Fill reusable outputs by providing them to the function.
		pub fn fill_outputs<F>(&self, _: F) where F: FnMut(usize, Output) {}
	}

	impl Decodable for Response {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			use ethcore::header::Header as FullHeader;
			use ethcore::transaction::UnverifiedTransaction;

			// check body validity.
			let _: Vec<FullHeader> = rlp.list_at(0)?;
			let _: Vec<UnverifiedTransaction> = rlp.list_at(1)?;

			Ok(Response {
				body: encoded::Body::new(rlp.as_raw().to_owned()),
			})
		}
	}

	impl Encodable for Response {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.append_raw(&self.body.rlp().as_raw(), 1);
		}
	}
}

/// A request for an account proof.
pub mod account {
	use super::{Field, NoSuchOutput, OutputKind, Output};
	use rlp::{Encodable, Decodable, DecoderError, RlpStream, UntrustedRlp};
	use util::{Bytes, U256, H256};

	/// Potentially incomplete request for an account proof.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Incomplete {
		/// Block hash to request state proof for.
		pub block_hash: Field<H256>,
		/// Hash of the account's address.
		pub address_hash: Field<H256>,
	}

	impl Decodable for Incomplete {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Incomplete {
				block_hash: rlp.val_at(0)?,
				address_hash: rlp.val_at(1)?,
			})
		}
	}

	impl Encodable for Incomplete {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(2)
				.append(&self.block_hash)
				.append(&self.address_hash);
		}
	}

	impl super::IncompleteRequest for Incomplete {
		type Complete = Complete;

		fn check_outputs<F>(&self, mut f: F) -> Result<(), NoSuchOutput>
			where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>
		{
			if let Field::BackReference(req, idx) = self.block_hash {
				f(req, idx, OutputKind::Hash)?
			}

			if let Field::BackReference(req, idx) = self.address_hash {
				f(req, idx, OutputKind::Hash)?
			}

			Ok(())
		}

		fn note_outputs<F>(&self, mut f: F) where F: FnMut(usize, OutputKind) {
			f(0, OutputKind::Hash);
			f(1, OutputKind::Hash);
		}

		fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput> {
			if let Field::BackReference(req, idx) = self.block_hash {
				self.block_hash = match oracle(req, idx) {
					Ok(Output::Hash(block_hash)) => Field::Scalar(block_hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}

			if let Field::BackReference(req, idx) = self.address_hash {
				self.address_hash = match oracle(req, idx) {
					Ok(Output::Hash(address_hash)) => Field::Scalar(address_hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}
		}

		fn complete(self) -> Result<Self::Complete, NoSuchOutput> {
			Ok(Complete {
				block_hash: self.block_hash.into_scalar()?,
				address_hash: self.address_hash.into_scalar()?,
			})
		}
	}

	/// A complete request for an account.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Complete {
		/// Block hash to request state proof for.
		pub block_hash: H256,
		/// Hash of the account's address.
		pub address_hash: H256,
	}

	/// The output of a request for an account state proof.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Response {
		/// Inclusion/exclusion proof
		pub proof: Vec<Bytes>,
		/// Account nonce.
		pub nonce: U256,
		/// Account balance.
		pub balance: U256,
		/// Account's code hash.
		pub code_hash: H256,
		/// Account's storage trie root.
		pub storage_root: H256,
	}

	impl Response {
		/// Fill reusable outputs by providing them to the function.
		pub fn fill_outputs<F>(&self, mut f: F) where F: FnMut(usize, Output) {
			f(0, Output::Hash(self.code_hash));
			f(1, Output::Hash(self.storage_root));
		}
	}

	impl Decodable for Response {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Response {
				proof: rlp.list_at(0)?,
				nonce: rlp.val_at(1)?,
				balance: rlp.val_at(2)?,
				code_hash: rlp.val_at(3)?,
				storage_root: rlp.val_at(4)?
			})
		}
	}

	impl Encodable for Response {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(5).begin_list(self.proof.len());
			for item in &self.proof {
				s.append_list(&item);
			}

			s.append(&self.nonce)
				.append(&self.balance)
				.append(&self.code_hash)
				.append(&self.storage_root);
		}
	}
}

/// A request for a storage proof.
pub mod storage {
	use super::{Field, NoSuchOutput, OutputKind, Output};
	use rlp::{Encodable, Decodable, DecoderError, RlpStream, UntrustedRlp};
	use util::{Bytes, H256};

	/// Potentially incomplete request for an storage proof.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Incomplete {
		/// Block hash to request state proof for.
		pub block_hash: Field<H256>,
		/// Hash of the account's address.
		pub address_hash: Field<H256>,
		/// Hash of the storage key.
		pub key_hash: Field<H256>,
	}

	impl Decodable for Incomplete {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Incomplete {
				block_hash: rlp.val_at(0)?,
				address_hash: rlp.val_at(1)?,
				key_hash: rlp.val_at(2)?,
			})
		}
	}

	impl Encodable for Incomplete {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(3)
				.append(&self.block_hash)
				.append(&self.address_hash)
				.append(&self.key_hash);
		}
	}

	impl super::IncompleteRequest for Incomplete {
		type Complete = Complete;

		fn check_outputs<F>(&self, mut f: F) -> Result<(), NoSuchOutput>
			where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>
		{
			if let Field::BackReference(req, idx) = self.block_hash {
				f(req, idx, OutputKind::Hash)?
			}

			if let Field::BackReference(req, idx) = self.address_hash {
				f(req, idx, OutputKind::Hash)?
			}

			if let Field::BackReference(req, idx) = self.key_hash {
				f(req, idx, OutputKind::Hash)?
			}

			Ok(())
		}

		fn note_outputs<F>(&self, mut f: F) where F: FnMut(usize, OutputKind) {
			f(0, OutputKind::Hash);
		}

		fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput> {
			if let Field::BackReference(req, idx) = self.block_hash {
				self.block_hash = match oracle(req, idx) {
					Ok(Output::Hash(block_hash)) => Field::Scalar(block_hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}

			if let Field::BackReference(req, idx) = self.address_hash {
				self.address_hash = match oracle(req, idx) {
					Ok(Output::Hash(address_hash)) => Field::Scalar(address_hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}

			if let Field::BackReference(req, idx) = self.key_hash {
				self.key_hash = match oracle(req, idx) {
					Ok(Output::Hash(key_hash)) => Field::Scalar(key_hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}
		}

		fn complete(self) -> Result<Self::Complete, NoSuchOutput> {
			Ok(Complete {
				block_hash: self.block_hash.into_scalar()?,
				address_hash: self.address_hash.into_scalar()?,
				key_hash: self.key_hash.into_scalar()?,
			})
		}
	}

	/// A complete request for a storage proof.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Complete {
		/// Block hash to request state proof for.
		pub block_hash: H256,
		/// Hash of the account's address.
		pub address_hash: H256,
		/// Storage key hash.
		pub key_hash: H256,
	}

	/// The output of a request for an account state proof.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Response {
		/// Inclusion/exclusion proof
		pub proof: Vec<Bytes>,
		/// Storage value.
		pub value: H256,
	}

	impl Response {
		/// Fill reusable outputs by providing them to the function.
		pub fn fill_outputs<F>(&self, mut f: F) where F: FnMut(usize, Output) {
			f(0, Output::Hash(self.value));
		}
	}

	impl Decodable for Response {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Response {
				proof: rlp.list_at(0)?,
				value: rlp.val_at(1)?,
			})
		}
	}

	impl Encodable for Response {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(2).begin_list(self.proof.len());
			for item in &self.proof {
				s.append_list(&item);
			}
			s.append(&self.value);
		}
	}
}

/// A request for contract code.
pub mod contract_code {
	use super::{Field, NoSuchOutput, OutputKind, Output};
	use rlp::{Encodable, Decodable, DecoderError, RlpStream, UntrustedRlp};
	use util::{Bytes, H256};

	/// Potentially incomplete contract code request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Incomplete {
		/// The block hash to request the state for.
		pub block_hash: Field<H256>,
		/// The code hash.
		pub code_hash: Field<H256>,
	}

	impl Decodable for Incomplete {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Incomplete {
				block_hash: rlp.val_at(0)?,
				code_hash: rlp.val_at(1)?,
			})
		}
	}

	impl Encodable for Incomplete {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(2)
				.append(&self.block_hash)
				.append(&self.code_hash);
		}
	}

	impl super::IncompleteRequest for Incomplete {
		type Complete = Complete;

		fn check_outputs<F>(&self, mut f: F) -> Result<(), NoSuchOutput>
			where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>
		{
			if let Field::BackReference(req, idx) = self.block_hash {
				f(req, idx, OutputKind::Hash)?;
			}
			if let Field::BackReference(req, idx) = self.code_hash {
				f(req, idx, OutputKind::Hash)?;
			}

			Ok(())
		}

		fn note_outputs<F>(&self, _: F) where F: FnMut(usize, OutputKind) {}

		fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput> {
			if let Field::BackReference(req, idx) = self.block_hash {
				self.block_hash = match oracle(req, idx) {
					Ok(Output::Hash(block_hash)) => Field::Scalar(block_hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}

			if let Field::BackReference(req, idx) = self.code_hash {
				self.code_hash = match oracle(req, idx) {
					Ok(Output::Hash(code_hash)) => Field::Scalar(code_hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}
		}

		fn complete(self) -> Result<Self::Complete, NoSuchOutput> {
			Ok(Complete {
				block_hash: self.block_hash.into_scalar()?,
				code_hash: self.code_hash.into_scalar()?,
			})
		}
	}

	/// A complete request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Complete {
		/// The block hash to request the state for.
		pub block_hash: H256,
		/// The code hash.
		pub code_hash: H256,
	}

	/// The output of a request for
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Response {
		/// The requested code.
		pub code: Bytes,
	}

	impl Response {
		/// Fill reusable outputs by providing them to the function.
		pub fn fill_outputs<F>(&self, _: F) where F: FnMut(usize, Output) {}
	}

	impl Decodable for Response {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {

			Ok(Response {
				code: rlp.as_val()?,
			})
		}
	}

	impl Encodable for Response {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.append(&self.code);
		}
	}
}

/// A request for proof of execution.
pub mod execution {
	use super::{Field, NoSuchOutput, OutputKind, Output};
	use ethcore::transaction::Action;
	use rlp::{Encodable, Decodable, DecoderError, RlpStream, UntrustedRlp};
	use util::{Bytes, Address, U256, H256, DBValue};

	/// Potentially incomplete execution proof request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Incomplete {
		/// The block hash to request the state for.
		pub block_hash: Field<H256>,
		/// The address the transaction should be from.
		pub from: Address,
		/// The action of the transaction.
		pub action: Action,
		/// The amount of gas to prove.
		pub gas: U256,
		/// The gas price.
		pub gas_price: U256,
		/// The value to transfer.
		pub value: U256,
		/// Call data.
		pub data: Bytes,
	}

	impl Decodable for Incomplete {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			Ok(Incomplete {
				block_hash: rlp.val_at(0)?,
				from: rlp.val_at(1)?,
				action: rlp.val_at(2)?,
				gas: rlp.val_at(3)?,
				gas_price: rlp.val_at(4)?,
				value: rlp.val_at(5)?,
				data: rlp.val_at(6)?,
			})
		}
	}

	impl Encodable for Incomplete {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(7)
				.append(&self.block_hash)
				.append(&self.from);

			match self.action {
				Action::Create => s.append_empty_data(),
				Action::Call(ref addr) => s.append(addr),
			};

			s.append(&self.gas)
				.append(&self.gas_price)
				.append(&self.value)
				.append(&self.data);
		}
	}

	impl super::IncompleteRequest for Incomplete {
		type Complete = Complete;

		fn check_outputs<F>(&self, mut f: F) -> Result<(), NoSuchOutput>
			where F: FnMut(usize, usize, OutputKind) -> Result<(), NoSuchOutput>
		{
			if let Field::BackReference(req, idx) = self.block_hash {
				f(req, idx, OutputKind::Hash)?;
			}

			Ok(())
		}

		fn note_outputs<F>(&self, _: F) where F: FnMut(usize, OutputKind) {}

		fn fill<F>(&mut self, oracle: F) where F: Fn(usize, usize) -> Result<Output, NoSuchOutput> {
			if let Field::BackReference(req, idx) = self.block_hash {
				self.block_hash = match oracle(req, idx) {
					Ok(Output::Hash(block_hash)) => Field::Scalar(block_hash.into()),
					_ => Field::BackReference(req, idx),
				}
			}
		}
		fn complete(self) -> Result<Self::Complete, NoSuchOutput> {
			Ok(Complete {
				block_hash: self.block_hash.into_scalar()?,
				from: self.from,
				action: self.action,
				gas: self.gas,
				gas_price: self.gas_price,
				value: self.value,
				data: self.data,
			})
		}
	}

	/// A complete request.
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Complete {
		/// The block hash to request the state for.
		pub block_hash: H256,
		/// The address the transaction should be from.
		pub from: Address,
		/// The action of the transaction.
		pub action: Action,
		/// The amount of gas to prove.
		pub gas: U256,
		/// The gas price.
		pub gas_price: U256,
		/// The value to transfer.
		pub value: U256,
		/// Call data.
		pub data: Bytes,
	}

	/// The output of a request for proof of execution
	#[derive(Debug, Clone, PartialEq, Eq)]
	pub struct Response {
		/// All state items (trie nodes, code) necessary to re-prove the transaction.
		pub items: Vec<DBValue>,
	}

	impl Response {
		/// Fill reusable outputs by providing them to the function.
		pub fn fill_outputs<F>(&self, _: F) where F: FnMut(usize, Output) {}
	}

	impl Decodable for Response {
		fn decode(rlp: &UntrustedRlp) -> Result<Self, DecoderError> {
			let mut items = Vec::new();
			for raw_item in rlp.iter() {
				let mut item = DBValue::new();
				item.append_slice(raw_item.data()?);
				items.push(item);
			}

			Ok(Response {
				items: items,
			})
		}
	}

	impl Encodable for Response {
		fn rlp_append(&self, s: &mut RlpStream) {
			s.begin_list(self.items.len());

			for item in &self.items {
				s.append(&&**item);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use ethcore::header::Header;

	fn check_roundtrip<T>(val: T)
		where T: ::rlp::Encodable + ::rlp::Decodable + PartialEq + ::std::fmt::Debug
	{
		let bytes = ::rlp::encode(&val);
		let new_val: T = ::rlp::decode(&bytes);
		assert_eq!(val, new_val);
	}

	#[test]
	fn hash_or_number_roundtrip() {
		let hash = HashOrNumber::Hash(H256::default());
		let number = HashOrNumber::Number(5);

		check_roundtrip(hash);
		check_roundtrip(number);
	}

	#[test]
	fn field_roundtrip() {
		let field_scalar = Field::Scalar(5usize);
		let field_back: Field<usize> = Field::BackReference(1, 2);

		check_roundtrip(field_scalar);
		check_roundtrip(field_back);
	}

	#[test]
	fn headers_roundtrip() {
		let req = IncompleteHeadersRequest {
			start: Field::Scalar(5u64.into()),
			skip: 0,
			max: 100,
			reverse: false,
		};

		let full_req = Request::Headers(req.clone());
		let res = HeadersResponse {
			headers: vec![
				::ethcore::encoded::Header::new(::rlp::encode(&Header::default()).to_vec())
			]
		};
		let full_res = Response::Headers(res.clone());

		check_roundtrip(req);
		check_roundtrip(full_req);
		check_roundtrip(res);
		check_roundtrip(full_res);
	}

	#[test]
	fn header_proof_roundtrip() {
		let req = IncompleteHeaderProofRequest {
			num: Field::BackReference(1, 234),
		};

		let full_req = Request::HeaderProof(req.clone());
		let res = HeaderProofResponse {
			proof: Vec::new(),
			hash: Default::default(),
			td: 100.into(),
		};
		let full_res = Response::HeaderProof(res.clone());

		check_roundtrip(req);
		check_roundtrip(full_req);
		check_roundtrip(res);
		check_roundtrip(full_res);
	}

	#[test]
	fn receipts_roundtrip() {
		let req = IncompleteReceiptsRequest {
			hash: Field::Scalar(Default::default()),
		};

		let full_req = Request::Receipts(req.clone());
		let res = ReceiptsResponse {
			receipts: vec![Default::default(), Default::default()],
		};
		let full_res = Response::Receipts(res.clone());

		check_roundtrip(req);
		check_roundtrip(full_req);
		check_roundtrip(res);
		check_roundtrip(full_res);
	}

	#[test]
	fn body_roundtrip() {
		let req = IncompleteBodyRequest {
			hash: Field::Scalar(Default::default()),
		};

		let full_req = Request::Body(req.clone());
		let res = BodyResponse {
			body: {
				let mut stream = RlpStream::new_list(2);
				stream.begin_list(0).begin_list(0);
				::ethcore::encoded::Body::new(stream.out())
			},
		};
		let full_res = Response::Body(res.clone());

		check_roundtrip(req);
		check_roundtrip(full_req);
		check_roundtrip(res);
		check_roundtrip(full_res);
	}

	#[test]
	fn account_roundtrip() {
		let req = IncompleteAccountRequest {
			block_hash: Field::Scalar(Default::default()),
			address_hash: Field::BackReference(1, 2),
		};

		let full_req = Request::Account(req.clone());
		let res = AccountResponse {
			proof: Vec::new(),
			nonce: 100.into(),
			balance: 123456.into(),
			code_hash: Default::default(),
			storage_root: Default::default(),
		};
		let full_res = Response::Account(res.clone());

		check_roundtrip(req);
		check_roundtrip(full_req);
		check_roundtrip(res);
		check_roundtrip(full_res);
	}

	#[test]
	fn storage_roundtrip() {
		let req = IncompleteStorageRequest {
			block_hash: Field::Scalar(Default::default()),
			address_hash: Field::BackReference(1, 2),
			key_hash: Field::BackReference(3, 2),
		};

		let full_req = Request::Storage(req.clone());
		let res = StorageResponse {
			proof: Vec::new(),
			value: H256::default(),
		};
		let full_res = Response::Storage(res.clone());

		check_roundtrip(req);
		check_roundtrip(full_req);
		check_roundtrip(res);
		check_roundtrip(full_res);
	}

	#[test]
	fn code_roundtrip() {
		let req = IncompleteCodeRequest {
			block_hash: Field::Scalar(Default::default()),
			code_hash: Field::BackReference(3, 2),
		};

		let full_req = Request::Code(req.clone());
		let res = CodeResponse {
			code: vec![1, 2, 3, 4, 5, 6, 7, 6, 5, 4],
		};
		let full_res = Response::Code(res.clone());

		check_roundtrip(req);
		check_roundtrip(full_req);
		check_roundtrip(res);
		check_roundtrip(full_res);
	}

	#[test]
	fn execution_roundtrip() {
		use util::DBValue;

		let req = IncompleteExecutionRequest {
			block_hash: Field::Scalar(Default::default()),
			from: Default::default(),
			action: ::ethcore::transaction::Action::Create,
			gas: 100_000.into(),
			gas_price: 0.into(),
			value: 100_000_001.into(),
			data: vec![1, 2, 3, 2, 1],
		};

		let full_req = Request::Execution(req.clone());
		let res = ExecutionResponse {
			items: vec![DBValue::new(), {
				let mut value = DBValue::new();
				value.append_slice(&[1, 1, 1, 2, 3]);
				value
			}],
		};
		let full_res = Response::Execution(res.clone());

		check_roundtrip(req);
		check_roundtrip(full_req);
		check_roundtrip(res);
		check_roundtrip(full_res);
	}

	#[test]
	fn vec_test() {
		use rlp::*;

		let reqs: Vec<_> = (0..10).map(|_| IncompleteExecutionRequest {
			block_hash: Field::Scalar(Default::default()),
			from: Default::default(),
			action: ::ethcore::transaction::Action::Create,
			gas: 100_000.into(),
			gas_price: 0.into(),
			value: 100_000_001.into(),
			data: vec![1, 2, 3, 2, 1],
		}).map(Request::Execution).collect();

		let mut stream = RlpStream::new_list(2);
		stream.append(&100usize).append_list(&reqs);
		let out = stream.out();

		let rlp = UntrustedRlp::new(&out);
		assert_eq!(rlp.val_at::<usize>(0).unwrap(), 100usize);
		assert_eq!(rlp.list_at::<Request>(1).unwrap(), reqs);
	}
}
