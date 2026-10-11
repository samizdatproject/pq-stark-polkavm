#!/bin/sh
# Benchmark matrix: proves each configuration, runs the blob in the local PolkaVM interpreter
# and, if ORIGIN is set (see dryrun.mjs), dry-runs it against live Kusama Asset Hub.
# Run ./build-contract.sh first. Arguments per row: log2(steps) queries blowup grinding.
set -e
cd "$(dirname "$0")/.."
BLOB=target/stark-verifier.polkavm
mkdir -p target/proofs
while read -r steps q b g; do
  f=target/proofs/fib-$steps-$q-$b-$g.bin
  ./target/release/host prove "$steps" "$q" "$b" "$g" "$f"
  ./target/release/host run "$BLOB" "$f" | tail -1
  if [ -n "$ORIGIN" ]; then
    (cd scripts && node dryrun.mjs "../$BLOB" "../$f" "$ORIGIN" 2>/dev/null | grep -E "^(statemine|verification only)")
  fi
done <<ROWS
10 27 8 16
10 28 16 16
10 20 32 16
16 27 8 16
20 27 8 16
10 40 8 0
ROWS
