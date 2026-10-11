#!/bin/sh
# Builds the verifier contract to target/stark-verifier.polkavm.
#
# Stable toolchain + RUSTC_BOOTSTRAP, as pallet-revive-fixtures does. The unroll threshold
# keeps every basic block under pallet-revive's 1,000-instruction limit (Winterfell's FRI
# folding, monomorphized for the cubic extension, otherwise produces a 1,337-instruction block).
#
# Pass FEATURES=sha3-in-vm to hash inside the VM instead of using the keccak host function.
set -e
cd "$(dirname "$0")"
F=${FEATURES:+--features air/$FEATURES}
RUSTFLAGS="-Cllvm-args=-unroll-threshold=100" RUSTC_BOOTSTRAP=1 \
  cargo build -p contract --release $F \
  --target contract/riscv64emac-unknown-none-polkavm.json \
  -Zbuild-std=core,alloc -Zjson-target-spec
cargo build -q --release -p host $F
./target/release/host link \
  target/riscv64emac-unknown-none-polkavm/release/stark-verifier target/stark-verifier.polkavm
./target/release/host check target/stark-verifier.polkavm
