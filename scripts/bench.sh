#!/bin/sh
# Benchmark matrix (every row of the README table): proves each configuration, runs the blob
# in the local PolkaVM interpreter and, if ORIGIN is set, dry-runs it against live Kusama
# Asset Hub. ORIGIN: any account mapped in pallet-revive holding >= ~0.2 KSM (see dryrun.mjs).
# Rows: log2(steps) queries blowup grinding. The last row is rebuilt with SHA3 inside the VM.
set -e
cd "$(dirname "$0")/.."
BLOB=target/stark-verifier.polkavm
mkdir -p target/proofs

bench() {
while read -r steps q b g; do
  f=target/proofs/fib-$steps-$q-$b-$g${FEATURES:+-$FEATURES}.bin
  ./target/release/host prove "$steps" "$q" "$b" "$g" "$f"
  ./target/release/host run "$BLOB" "$f" | tail -1
  if [ -n "$ORIGIN" ]; then
    (cd scripts && node dryrun.mjs "../$BLOB" "../$f" "$ORIGIN" 2>/dev/null | grep -E "^(statemine|verification only)")
  fi
done
}

./build-contract.sh >/dev/null 2>&1
printf '10 27 8 16\n10 20 32 16\n10 40 8 0\n10 28 16 16\n16 27 8 16\n20 27 8 16\n' | bench
echo "--- SHA3 inside the VM"
export FEATURES=sha3-in-vm
./build-contract.sh >/dev/null 2>&1
echo '10 27 8 16' | bench
unset FEATURES
./build-contract.sh >/dev/null 2>&1
