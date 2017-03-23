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

use std::cmp::{max, min};
use std::io::{self, Read};

use byteorder::{ByteOrder, BigEndian};
use crypto::sha2::Sha256 as Sha256Digest;
use crypto::ripemd160::Ripemd160 as Ripemd160Digest;
use crypto::digest::Digest;
use num::{BigUint, Zero, One};

use util::{U256, H256, Uint, Hashable, BytesRef};
use ethkey::{Signature, recover as ec_recover};
use ethjson;

/// Native implementation of a built-in contract.
pub trait Impl: Send + Sync {
	/// execute this built-in on the given input, writing to the given output.
	fn execute(&self, input: &[u8], output: &mut BytesRef);
}

/// A gas pricing scheme for built-in contracts.
pub trait Pricer: Send + Sync {
	/// The gas cost of running this built-in for the given input data.
	fn cost(&self, input: &[u8]) -> U256;
}

/// A linear pricing model. This computes a price using a base cost and a cost per-word.
struct Linear {
	base: usize,
	word: usize,
}

/// A special pricing model for modular exponentiation.
struct Modexp {
	divisor: usize,
}

impl Pricer for Linear {
	fn cost(&self, input: &[u8]) -> U256 {
		U256::from(self.base) + U256::from(self.word) * U256::from((input.len() + 31) / 32)
	}
}

impl Pricer for Modexp {
	fn cost(&self, input: &[u8]) -> U256 {
		let mut reader = input.chain(io::repeat(0));
		let mut buf = [0; 32];

		// read lengths as U256 here for accurate gas calculation.
		let mut read_len = || {
			reader.read_exact(&mut buf[..]).expect("reading from zero-extended memory cannot fail; qed");
			U256::from(H256::from_slice(&buf[..]))
		};
		let base_len = read_len();
		let exp_len = read_len();
		let mod_len = read_len();

		// floor(max(length_of_MODULUS, length_of_BASE) ** 2 * max(length_of_EXPONENT, 1) / GQUADDIVISOR)
		// TODO: is saturating the best behavior here?
		let m = max(mod_len, base_len);
		match m.overflowing_mul(m) {
			(_, true) => U256::max_value(),
			(val, _) => {
				match val.overflowing_mul(max(exp_len, U256::one())) {
					(_, true) => U256::max_value(),
					(val, _) => val / (self.divisor as u64).into()
				}
			}
		}
	}
}

/// Pricing scheme, execution definition, and activation block for a built-in contract.
///
/// Call `cost` to compute cost for the given input, `execute` to execute the contract
/// on the given input, and `is_active` to determine whether the contract is active.
///
/// Unless `is_active` is true,
pub struct Builtin {
	pricer: Box<Pricer>,
	native: Box<Impl>,
	activate_at: u64,
}

impl Builtin {
	/// Simple forwarder for cost.
	pub fn cost(&self, input: &[u8]) -> U256 { self.pricer.cost(input) }

	/// Simple forwarder for execute.
	pub fn execute(&self, input: &[u8], output: &mut BytesRef) { self.native.execute(input, output) }

	/// Whether the builtin is activated at the given block number.
	pub fn is_active(&self, at: u64) -> bool { at >= self.activate_at }
}

impl From<ethjson::spec::Builtin> for Builtin {
	fn from(b: ethjson::spec::Builtin) -> Self {
		let pricer: Box<Pricer> = match b.pricing {
			ethjson::spec::Pricing::Linear(linear) => {
				Box::new(Linear {
					base: linear.base,
					word: linear.word,
				})
			}
			ethjson::spec::Pricing::Modexp(exp) => {
				Box::new(Modexp {
					divisor: if exp.divisor == 0 {
						warn!("Zero modexp divisor specified. Falling back to default.");
						10
					} else {
						exp.divisor
					}
				})
			}
		};

		Builtin {
			pricer: pricer,
			native: ethereum_builtin(&b.name),
			activate_at: b.activate_at.map(Into::into).unwrap_or(0),
		}
	}
}

// Ethereum builtin creator.
fn ethereum_builtin(name: &str) -> Box<Impl> {
	match name {
		"identity" => Box::new(Identity) as Box<Impl>,
		"ecrecover" => Box::new(EcRecover) as Box<Impl>,
		"sha256" => Box::new(Sha256) as Box<Impl>,
		"ripemd160" => Box::new(Ripemd160) as Box<Impl>,
		"modexp" => Box::new(ModexpImpl) as Box<Impl>,
		_ => panic!("invalid builtin name: {}", name),
	}
}

// Ethereum builtins:
//
// - The identity function
// - ec recovery
// - sha256
// - ripemd160
// - modexp (EIP198)

#[derive(Debug)]
struct Identity;

#[derive(Debug)]
struct EcRecover;

#[derive(Debug)]
struct Sha256;

#[derive(Debug)]
struct Ripemd160;

#[derive(Debug)]
struct ModexpImpl;

impl Impl for Identity {
	fn execute(&self, input: &[u8], output: &mut BytesRef) {
		output.write(0, input);
	}
}

impl Impl for EcRecover {
	fn execute(&self, i: &[u8], output: &mut BytesRef) {
		let len = min(i.len(), 128);

		let mut input = [0; 128];
		input[..len].copy_from_slice(&i[..len]);

		let hash = H256::from_slice(&input[0..32]);
		let v = H256::from_slice(&input[32..64]);
		let r = H256::from_slice(&input[64..96]);
		let s = H256::from_slice(&input[96..128]);

		let bit = match v[31] {
			27 | 28 if &v.0[..31] == &[0; 31] => v[31] - 27,
			_ => return,
		};

		let s = Signature::from_rsv(&r, &s, bit);
		if s.is_valid() {
			if let Ok(p) = ec_recover(&s, &hash) {
				let r = p.sha3();
				output.write(0, &[0; 12]);
				output.write(12, &r[12..r.len()]);
			}
		}
	}
}

impl Impl for Sha256 {
	fn execute(&self, input: &[u8], output: &mut BytesRef) {
		let mut sha = Sha256Digest::new();
		sha.input(input);

		let mut out = [0; 32];
		sha.result(&mut out);

		output.write(0, &out);
	}
}

impl Impl for Ripemd160 {
	fn execute(&self, input: &[u8], output: &mut BytesRef) {
		let mut sha = Ripemd160Digest::new();
		sha.input(input);

		let mut out = [0; 32];
		sha.result(&mut out[12..32]);

		output.write(0, &out);
	}
}

impl Impl for ModexpImpl {
	fn execute(&self, input: &[u8], output: &mut BytesRef) {
		let mut reader = input.chain(io::repeat(0));
		let mut buf = [0; 32];

		// read lengths as usize.
		// ignoring the first 24 bytes might technically lead us to fall out of consensus,
		// but so would running out of addressable memory!
		let mut read_len = |reader: &mut io::Chain<&[u8], io::Repeat>| {
			reader.read_exact(&mut buf[..]).expect("reading from zero-extended memory cannot fail; qed");
			BigEndian::read_u64(&buf[24..]) as usize
		};

		let base_len = read_len(&mut reader);
		let exp_len = read_len(&mut reader);
		let mod_len = read_len(&mut reader);

		// read the numbers themselves.
		let mut buf = vec![0; max(mod_len, max(base_len, exp_len))];
		let mut read_num = |len| {
			reader.read_exact(&mut buf[..len]).expect("reading from zero-extended memory cannot fail; qed");
			BigUint::from_bytes_be(&buf[..len])
		};

		let base = read_num(base_len);
		let exp = read_num(exp_len);
		let modulus = read_num(mod_len);

		// calculate modexp: exponentiation by squaring.
		fn modexp(mut base: BigUint, mut exp: BigUint, modulus: BigUint) -> BigUint {
			match (base == BigUint::zero(), exp == BigUint::zero()) {
				(_, true) => return BigUint::one(), // n^0 % m
				(true, false) => return BigUint::zero(), // 0^n % m, n>0
				(false, false) if modulus <= BigUint::one() => return BigUint::zero(), // a^b % 1 = 0.
				_ => {}
			}

			let mut result = BigUint::one();
			base = base % &modulus;

			// fast path for base divisible by modulus.
			if base == BigUint::zero() { return result }
			while exp != BigUint::zero() {
				// exp has to be on the right here to avoid move.
				if BigUint::one() & &exp == BigUint::one() {
					result = (result * &base) % &modulus;
				}

				exp = exp >> 1;
				base = (base.clone() * base) % &modulus;
			}

			result
		}

		// write output to given memory, left padded and same length as the modulus.
		let bytes = modexp(base, exp, modulus).to_bytes_be();

		// always true except in the case of zero-length modulus, which leads to
		// output of length and value 1.
		if bytes.len() <= mod_len {
			let res_start = mod_len - bytes.len();
			output.write(res_start, &bytes);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{Builtin, Linear, ethereum_builtin, Pricer, Modexp};
	use ethjson;
	use util::{U256, BytesRef};

	#[test]
	fn identity() {
		let f = ethereum_builtin("identity");

		let i = [0u8, 1, 2, 3];

		let mut o2 = [255u8; 2];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o2[..]));
		assert_eq!(i[0..2], o2);

		let mut o4 = [255u8; 4];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o4[..]));
		assert_eq!(i, o4);

		let mut o8 = [255u8; 8];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o8[..]));
		assert_eq!(i, o8[..4]);
		assert_eq!([255u8; 4], o8[4..]);
	}

	#[test]
	fn sha256() {
		use rustc_serialize::hex::FromHex;
		let f = ethereum_builtin("sha256");

		let i = [0u8; 0];

		let mut o = [255u8; 32];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap())[..]);

		let mut o8 = [255u8; 8];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o8[..]));
		assert_eq!(&o8[..], &(FromHex::from_hex("e3b0c44298fc1c14").unwrap())[..]);

		let mut o34 = [255u8; 34];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o34[..]));
		assert_eq!(&o34[..], &(FromHex::from_hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855ffff").unwrap())[..]);

		let mut ov = vec![];
		f.execute(&i[..], &mut BytesRef::Flexible(&mut ov));
		assert_eq!(&ov[..], &(FromHex::from_hex("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap())[..]);
	}

	#[test]
	fn ripemd160() {
		use rustc_serialize::hex::FromHex;
		let f = ethereum_builtin("ripemd160");

		let i = [0u8; 0];

		let mut o = [255u8; 32];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("0000000000000000000000009c1185a5c5e9fc54612808977ee8f548b2258d31").unwrap())[..]);

		let mut o8 = [255u8; 8];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o8[..]));
		assert_eq!(&o8[..], &(FromHex::from_hex("0000000000000000").unwrap())[..]);

		let mut o34 = [255u8; 34];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o34[..]));
		assert_eq!(&o34[..], &(FromHex::from_hex("0000000000000000000000009c1185a5c5e9fc54612808977ee8f548b2258d31ffff").unwrap())[..]);
	}

	#[test]
	fn ecrecover() {
		use rustc_serialize::hex::FromHex;
		/*let k = KeyPair::from_secret(b"test".sha3()).unwrap();
		let a: Address = From::from(k.public().sha3());
		println!("Address: {}", a);
		let m = b"hello world".sha3();
		println!("Message: {}", m);
		let s = k.sign(&m).unwrap();
		println!("Signed: {}", s);*/

		let f = ethereum_builtin("ecrecover");

		let i = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b650acf9d3f5f0a2c799776a1254355d5f4061762a237396a99a0e0e3fc2bcd6729514a0dacb2e623ac4abd157cb18163ff942280db4d5caad66ddf941ba12e03").unwrap();

		let mut o = [255u8; 32];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("000000000000000000000000c08b5542d177ac6686946920409741463a15dddb").unwrap())[..]);

		let mut o8 = [255u8; 8];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o8[..]));
		assert_eq!(&o8[..], &(FromHex::from_hex("0000000000000000").unwrap())[..]);

		let mut o34 = [255u8; 34];
		f.execute(&i[..], &mut BytesRef::Fixed(&mut o34[..]));
		assert_eq!(&o34[..], &(FromHex::from_hex("000000000000000000000000c08b5542d177ac6686946920409741463a15dddbffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001a650acf9d3f5f0a2c799776a1254355d5f4061762a237396a99a0e0e3fc2bcd6729514a0dacb2e623ac4abd157cb18163ff942280db4d5caad66ddf941ba12e03").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b000000000000000000000000000000000000000000000000000000000000001b0000000000000000000000000000000000000000000000000000000000000000").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001b").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001bffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff000000000000000000000000000000000000000000000000000000000000001b").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		let i_bad = FromHex::from_hex("47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b000000000000000000000000000000000000000000000000000000000000001bffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);

		// TODO: Should this (corrupted version of the above) fail rather than returning some address?
	/*	let i_bad = FromHex::from_hex("48173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad000000000000000000000000000000000000000000000000000000000000001b650acf9d3f5f0a2c799776a1254355d5f4061762a237396a99a0e0e3fc2bcd6729514a0dacb2e623ac4abd157cb18163ff942280db4d5caad66ddf941ba12e03").unwrap();
		let mut o = [255u8; 32];
		f.execute(&i_bad[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(&o[..], &(FromHex::from_hex("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff").unwrap())[..]);*/
	}

	#[test]
	fn modexp() {
		use rustc_serialize::hex::FromHex;

		let f = Builtin {
			pricer: Box::new(Modexp { divisor: 20 }),
			native: ethereum_builtin("modexp"),
			activate_at: 0,
		};
		// fermat's little theorem example.
		{
			let input = FromHex::from_hex("\
				0000000000000000000000000000000000000000000000000000000000000001\
				0000000000000000000000000000000000000000000000000000000000000020\
				0000000000000000000000000000000000000000000000000000000000000020\
				03\
				fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2e\
				fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f"
			).unwrap();

			let mut output = vec![0u8; 32];
			let expected = FromHex::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
			let expected_cost = 1638;

			f.execute(&input[..], &mut BytesRef::Fixed(&mut output[..]));
			assert_eq!(output, expected);
			assert_eq!(f.cost(&input[..]), expected_cost.into());
		}

		// second example from EIP: zero base.
		{
			let input = FromHex::from_hex("\
				0000000000000000000000000000000000000000000000000000000000000000\
 				0000000000000000000000000000000000000000000000000000000000000020\
 				0000000000000000000000000000000000000000000000000000000000000020\
 				fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2e\
 				fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f"
			).unwrap();

			let mut output = vec![0u8; 32];
			let expected = FromHex::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
			let expected_cost = 1638;

			f.execute(&input[..], &mut BytesRef::Fixed(&mut output[..]));
			assert_eq!(output, expected);
			assert_eq!(f.cost(&input[..]), expected_cost.into());
		}

		// another example from EIP: zero-padding
		{
			let input = FromHex::from_hex("\
				0000000000000000000000000000000000000000000000000000000000000001\
				0000000000000000000000000000000000000000000000000000000000000002\
				0000000000000000000000000000000000000000000000000000000000000020\
				03\
				ffff\
				80"
			).unwrap();

			let mut output = vec![0u8; 32];
			let expected = FromHex::from_hex("3b01b01ac41f2d6e917c6d6a221ce793802469026d9ab7578fa2e79e4da6aaab").unwrap();
			let expected_cost = 102;

			f.execute(&input[..], &mut BytesRef::Fixed(&mut output[..]));
			assert_eq!(output, expected);
			assert_eq!(f.cost(&input[..]), expected_cost.into());
		}

		// zero-length modulus.
		{
			let input = FromHex::from_hex("\
				0000000000000000000000000000000000000000000000000000000000000001\
				0000000000000000000000000000000000000000000000000000000000000002\
				0000000000000000000000000000000000000000000000000000000000000000\
				03\
				ffff"
			).unwrap();

			let mut output = vec![];
			let expected_cost = 0;

			f.execute(&input[..], &mut BytesRef::Flexible(&mut output));
			assert_eq!(output.len(), 0); // shouldn't have written any output.
			assert_eq!(f.cost(&input[..]), expected_cost.into());
		}
	}

	#[test]
	#[should_panic]
	fn from_unknown_linear() {
		let _ = ethereum_builtin("foo");
	}

	#[test]
	fn is_active() {
		let pricer = Box::new(Linear { base: 10, word: 20} );
		let b = Builtin {
			pricer: pricer as Box<Pricer>,
			native: ethereum_builtin("identity"),
			activate_at: 100_000,
		};

		assert!(!b.is_active(99_999));
		assert!(b.is_active(100_000));
		assert!(b.is_active(100_001));
	}

	#[test]
	fn from_named_linear() {
		let pricer = Box::new(Linear { base: 10, word: 20 });
		let b = Builtin {
			pricer: pricer as Box<Pricer>,
			native: ethereum_builtin("identity"),
			activate_at: 1,
		};

		assert_eq!(b.cost(&[0; 0]), U256::from(10));
		assert_eq!(b.cost(&[0; 1]), U256::from(30));
		assert_eq!(b.cost(&[0; 32]), U256::from(30));
		assert_eq!(b.cost(&[0; 33]), U256::from(50));

		let i = [0u8, 1, 2, 3];
		let mut o = [255u8; 4];
		b.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(i, o);
	}

	#[test]
	fn from_json() {
		let b = Builtin::from(ethjson::spec::Builtin {
			name: "identity".to_owned(),
			pricing: ethjson::spec::Pricing::Linear(ethjson::spec::Linear {
				base: 10,
				word: 20,
			}),
			activate_at: None,
		});

		assert_eq!(b.cost(&[0; 0]), U256::from(10));
		assert_eq!(b.cost(&[0; 1]), U256::from(30));
		assert_eq!(b.cost(&[0; 32]), U256::from(30));
		assert_eq!(b.cost(&[0; 33]), U256::from(50));

		let i = [0u8, 1, 2, 3];
		let mut o = [255u8; 4];
		b.execute(&i[..], &mut BytesRef::Fixed(&mut o[..]));
		assert_eq!(i, o);
	}
}
