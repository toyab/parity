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

//! Traces api implementation.

use std::sync::{Weak, Arc};

use rlp::UntrustedRlp;
use ethcore::client::{MiningBlockChainClient, CallAnalytics, TransactionId, TraceId};
use ethcore::miner::MinerService;
use ethcore::transaction::SignedTransaction;

use jsonrpc_core::Error;
use jsonrpc_macros::Trailing;
use v1::traits::Traces;
use v1::helpers::{errors, fake_sign};
use v1::types::{TraceFilter, LocalizedTrace, BlockNumber, Index, CallRequest, Bytes, TraceResults, H256};

fn to_call_analytics(flags: Vec<String>) -> CallAnalytics {
	CallAnalytics {
		transaction_tracing: flags.contains(&("trace".to_owned())),
		vm_tracing: flags.contains(&("vmTrace".to_owned())),
		state_diffing: flags.contains(&("stateDiff".to_owned())),
	}
}

/// Traces api implementation.
pub struct TracesClient<C, M> {
	client: Weak<C>,
	miner: Weak<M>,
}

impl<C, M> TracesClient<C, M> {
	/// Creates new Traces client.
	pub fn new(client: &Arc<C>, miner: &Arc<M>) -> Self {
		TracesClient {
			client: Arc::downgrade(client),
			miner: Arc::downgrade(miner),
		}
	}
}

impl<C, M> Traces for TracesClient<C, M> where C: MiningBlockChainClient + 'static, M: MinerService + 'static {
	fn filter(&self, filter: TraceFilter) -> Result<Option<Vec<LocalizedTrace>>, Error> {
		Ok(take_weak!(self.client).filter_traces(filter.into())
			.map(|traces| traces.into_iter().map(LocalizedTrace::from).collect()))
	}

	fn block_traces(&self, block_number: BlockNumber) -> Result<Option<Vec<LocalizedTrace>>, Error> {
		Ok(take_weak!(self.client).block_traces(block_number.into())
			.map(|traces| traces.into_iter().map(LocalizedTrace::from).collect()))
	}

	fn transaction_traces(&self, transaction_hash: H256) -> Result<Option<Vec<LocalizedTrace>>, Error> {
		Ok(take_weak!(self.client).transaction_traces(TransactionId::Hash(transaction_hash.into()))
			.map(|traces| traces.into_iter().map(LocalizedTrace::from).collect()))
	}

	fn trace(&self, transaction_hash: H256, address: Vec<Index>) -> Result<Option<LocalizedTrace>, Error> {
		let id = TraceId {
			transaction: TransactionId::Hash(transaction_hash.into()),
			address: address.into_iter().map(|i| i.value()).collect()
		};

		Ok(take_weak!(self.client).trace(id)
			.map(LocalizedTrace::from))
	}

	fn call(&self, request: CallRequest, flags: Vec<String>, block: Trailing<BlockNumber>) -> Result<TraceResults, Error> {
		let block = block.0;

		let request = CallRequest::into(request);
		let signed = fake_sign::sign_call(&self.client, &self.miner, request)?;

		take_weak!(self.client).call(&signed, block.into(), to_call_analytics(flags))
			.map(TraceResults::from)
			.map_err(errors::from_call_error)
	}

	fn raw_transaction(&self, raw_transaction: Bytes, flags: Vec<String>, block: Trailing<BlockNumber>) -> Result<TraceResults, Error> {
		let block = block.0;

		let tx = UntrustedRlp::new(&raw_transaction.into_vec()).as_val().map_err(|e| errors::invalid_params("Transaction is not valid RLP", e))?;
		let signed = SignedTransaction::new(tx).map_err(errors::from_transaction_error)?;

		take_weak!(self.client).call(&signed, block.into(), to_call_analytics(flags))
			.map(TraceResults::from)
			.map_err(errors::from_call_error)
	}

	fn replay_transaction(&self, transaction_hash: H256, flags: Vec<String>) -> Result<TraceResults, Error> {
		take_weak!(self.client).replay(TransactionId::Hash(transaction_hash.into()), to_call_analytics(flags))
			.map(TraceResults::from)
			.map_err(errors::from_call_error)
	}
}
