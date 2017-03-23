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

/// Validator lists.

mod simple_list;
mod safe_contract;
mod contract;
mod multi;

use std::sync::Weak;
use util::{Address, H256};
use ethjson::spec::ValidatorSet as ValidatorSpec;
use client::Client;
use self::simple_list::SimpleList;
use self::contract::ValidatorContract;
use self::safe_contract::ValidatorSafeContract;
use self::multi::Multi;

/// Creates a validator set from spec.
pub fn new_validator_set(spec: ValidatorSpec) -> Box<ValidatorSet> {
	match spec {
		ValidatorSpec::List(list) => Box::new(SimpleList::new(list.into_iter().map(Into::into).collect())),
		ValidatorSpec::SafeContract(address) => Box::new(ValidatorSafeContract::new(address.into())),
		ValidatorSpec::Contract(address) => Box::new(ValidatorContract::new(address.into())),
		ValidatorSpec::Multi(sequence) => Box::new(
			Multi::new(sequence.into_iter().map(|(block, set)| (block.into(), new_validator_set(set))).collect())
		),
	}
}

pub trait ValidatorSet: Send + Sync {
	/// Checks if a given address is a validator.
	fn contains(&self, parent_block_hash: &H256, address: &Address) -> bool;
	/// Draws an validator nonce modulo number of validators.
	fn get(&self, parent_block_hash: &H256, nonce: usize) -> Address;
	/// Returns the current number of validators.
	fn count(&self, parent_block_hash: &H256) -> usize;
	/// Notifies about malicious behaviour.
	fn report_malicious(&self, _validator: &Address) {}
	/// Notifies about benign misbehaviour.
	fn report_benign(&self, _validator: &Address) {}
	/// Allows blockchain state access.
	fn register_contract(&self, _client: Weak<Client>) {}
}
