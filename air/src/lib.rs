//! The statement being proven, shared by the off-chain prover and the on-chain verifier.
//!
//! "I know the Fibonacci sequence starting at (1, 1) whose term number `2 * trace_len` is
//! `result`" over the Goldilocks field. It is the standard Winterfell `fib2` AIR: two columns,
//! each row advances the sequence by two terms. It is deliberately trivial so that the
//! measured cost is the cost of the STARK/FRI verifier itself, not of a complex AIR.
#![no_std]

extern crate alloc;

use alloc::{vec, vec::Vec};
use winter_air::{
    Air, AirContext, Assertion, EvaluationFrame, TraceInfo, TransitionConstraintDegree,
};
pub use winter_air::{BatchingMethod, FieldExtension, ProofOptions};
pub use winter_math::{FieldElement, fields::f64::BaseElement};

mod keccak;

/// Hash used for Merkle commitments and Fiat-Shamir. By default Keccak-256 via the
/// pallet-revive host function; with the `sha3-in-vm` feature, Winterfell's pure-Rust SHA3-256
/// runs inside PolkaVM instead (kept for comparison). Winterfell's Blake3 cannot be used:
/// its compression function is a single 1,329-instruction basic block, above pallet-revive's
/// 1,000-instruction limit, so the blob is rejected at upload.
#[cfg(not(feature = "sha3-in-vm"))]
pub type Hasher = keccak::Keccak256<BaseElement>;
#[cfg(feature = "sha3-in-vm")]
pub type Hasher = winter_crypto::hashers::Sha3_256<BaseElement>;
pub type VectorCommitment = winter_crypto::MerkleTree<Hasher>;
pub type RandomCoin = winter_crypto::DefaultRandomCoin<Hasher>;

/// The verifier rejects any proof whose conjectured security is below this many bits.
pub const MIN_SECURITY_BITS: u32 = 96;

pub const TRACE_WIDTH: usize = 2;

/// Proof parameters. Only the knobs worth benchmarking are exposed; the rest are fixed.
pub fn proof_options(num_queries: usize, blowup: usize, grinding_bits: u32) -> ProofOptions {
    ProofOptions::new(
        num_queries,
        blowup,
        grinding_bits,
        // Goldilocks is 64-bit; a quadratic extension is needed for ~128-bit soundness.
        FieldExtension::Quadratic,
        8,  // FRI folding factor
        31, // FRI remainder max degree
        BatchingMethod::Linear,
        BatchingMethod::Linear,
    )
}

pub struct FibAir {
    context: AirContext<BaseElement>,
    result: BaseElement,
}

impl Air for FibAir {
    type BaseField = BaseElement;
    type PublicInputs = BaseElement;

    fn new(trace_info: TraceInfo, result: BaseElement, options: ProofOptions) -> Self {
        let degrees = vec![
            TransitionConstraintDegree::new(1),
            TransitionConstraintDegree::new(1),
        ];
        FibAir {
            context: AirContext::new(trace_info, degrees, 3, options),
            result,
        }
    }

    fn context(&self) -> &AirContext<BaseElement> {
        &self.context
    }

    fn evaluate_transition<E: FieldElement<BaseField = BaseElement>>(
        &self,
        frame: &EvaluationFrame<E>,
        _periodic_values: &[E],
        result: &mut [E],
    ) {
        let (cur, next) = (frame.current(), frame.next());
        result[0] = next[0] - (cur[0] + cur[1]);
        result[1] = next[1] - (cur[1] + next[0]);
    }

    fn get_assertions(&self) -> Vec<Assertion<BaseElement>> {
        let last = self.trace_length() - 1;
        vec![
            Assertion::single(0, 0, BaseElement::ONE),
            Assertion::single(1, 0, BaseElement::ONE),
            Assertion::single(1, last, self.result),
        ]
    }
}
