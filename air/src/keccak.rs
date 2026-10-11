//! Keccak-256 hasher for Winterfell.
//!
//! Inside PolkaVM the hash is computed by pallet-revive's `hash_keccak_256` host function,
//! which is charged a flat weight plus a per-byte fee instead of the ~60% of all executed
//! instructions that an in-VM Keccak-f permutation costs. Off-chain (the prover) the same
//! function comes from the `sha3` crate, so both sides compute identical digests.

use alloc::vec::Vec;
use core::marker::PhantomData;
use winter_crypto::{Digest, ElementHasher, Hasher, hashers::Sha3_256};
use winter_math::{FieldElement, StarkField, fields::f64::BaseElement};

// winter-crypto does not export its `ByteDigest<32>` type, but it is reachable (with its
// public constructor) as the digest type of one of its own hashers.
type ByteDigest = <Sha3_256<BaseElement> as Hasher>::Digest;

pub struct Keccak256<B>(PhantomData<B>);

#[cfg(target_arch = "riscv64")]
fn keccak(data: &[u8]) -> [u8; 32] {
    use uapi::{HostFn, HostFnImpl as api};
    let mut out = [0u8; 32];
    api::hash_keccak_256(data, &mut out);
    out
}

#[cfg(not(target_arch = "riscv64"))]
fn keccak(data: &[u8]) -> [u8; 32] {
    use sha3::Digest;
    sha3::Keccak256::digest(data).into()
}

impl<B: StarkField> Hasher for Keccak256<B> {
    type Digest = ByteDigest;
    const COLLISION_RESISTANCE: u32 = 128;

    fn hash(bytes: &[u8]) -> Self::Digest {
        ByteDigest::new(keccak(bytes))
    }

    fn merge(values: &[Self::Digest; 2]) -> Self::Digest {
        Self::hash(ByteDigest::digests_as_bytes(values))
    }

    fn merge_many(values: &[Self::Digest]) -> Self::Digest {
        Self::hash(ByteDigest::digests_as_bytes(values))
    }

    fn merge_with_int(seed: Self::Digest, value: u64) -> Self::Digest {
        let mut data = [0; 40];
        data[..32].copy_from_slice(seed.as_bytes().as_slice());
        data[32..].copy_from_slice(&value.to_le_bytes());
        Self::hash(&data)
    }
}

impl<B: StarkField> ElementHasher for Keccak256<B> {
    type BaseField = B;

    fn hash_elements<E: FieldElement<BaseField = B>>(elements: &[E]) -> Self::Digest {
        // Goldilocks elements are stored in Montgomery form, so serialize them canonically
        // first (this mirrors what Winterfell's Sha3_256 does for non-canonical fields).
        let mut bytes = Vec::with_capacity(elements.len() * E::ELEMENT_BYTES);
        elements.iter().for_each(|e| e.write_into(&mut bytes));
        Self::hash(&bytes)
    }
}
