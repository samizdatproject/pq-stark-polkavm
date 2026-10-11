//! STARK verifier as a native PolkaVM contract for pallet-revive.
//!
//! Call data: `result: u64 (little endian) || winterfell proof bytes`.
//! Returns `[1]` if the proof verifies; reverts with `[0]` otherwise.
//!
//! The verifier is stateless: it reads call data, verifies, and returns. It runs in both
//! `deploy` and `call`, so the same blob can be measured with an instantiate dry-run
//! (no deployment needed) or called after deployment.
#![no_std]
#![no_main]

extern crate alloc;

use air::{BaseElement, FibAir, MIN_SECURITY_BITS};
use alloc::vec;
use uapi::{HostFn, HostFnImpl as api, ReturnFlags};
use winter_verifier::{AcceptableOptions, Proof};

mod bump;

#[global_allocator]
static ALLOC: bump::Bump = bump::Bump;

// The verifier recurses through deserialization and FRI layers; the default stack is too small.
// Stack and heap both count towards pallet-revive's static memory limit.
polkavm_derive::min_stack_size!(256 * 1024);

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // `unimp` traps; pallet-revive turns the trap into a failed call.
    unsafe {
        core::arch::asm!("unimp");
        core::hint::unreachable_unchecked()
    }
}

fn verify() -> bool {
    let len = api::call_data_size() as usize;
    if len < 8 {
        return false;
    }
    let mut input = vec![0u8; len];
    api::call_data_copy(&mut input, 0);

    let result = u64::from_le_bytes(input[..8].try_into().unwrap());
    let Ok(proof) = Proof::from_bytes(&input[8..]) else {
        return false;
    };
    winter_verifier::verify::<FibAir, air::Hasher, air::RandomCoin, air::VectorCommitment>(
        proof,
        BaseElement::new(result),
        &AcceptableOptions::MinConjecturedSecurity(MIN_SECURITY_BITS),
    )
    .is_ok()
}

fn verify_and_return() {
    // An empty call (e.g. a plain deploy) succeeds without doing anything.
    if api::call_data_size() == 0 {
        return;
    }
    let ok = verify();
    let flags = if ok {
        ReturnFlags::empty()
    } else {
        ReturnFlags::REVERT
    };
    api::return_value(flags, &[ok as u8]);
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn deploy() {
    verify_and_return()
}

#[no_mangle]
#[polkavm_derive::polkavm_export]
pub extern "C" fn call() {
    verify_and_return()
}
