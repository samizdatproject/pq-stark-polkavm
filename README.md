# pq-stark-polkavm

A hash-based STARK verifier running as a native Rust contract on PolkaVM (pallet-revive),
targeting Kusama Asset Hub. This is a feasibility spike, not production code.

The verifier is [Winterfell](https://github.com/facebook/winterfell) 0.13.1 (`no_std`),
compiled to a `ReviveV1` PolkaVM blob. Hashing goes through pallet-revive's `hash_keccak_256`
host function. The scheme is hash-based and needs no trusted setup. Its post-quantum security
is subject to the caveat under [Security levels](#security-levels).

**Result.** Verification fits within a single call's weight budget. This was measured with a
dry-run on the live Kusama Asset Hub runtime; no transaction was submitted.
- The smallest configuration costs 2.0e11 ref_time. That is about 4–7x a Groth16 verifier
  backed by the BN254 precompiles (see [Comparison with Groth16](#comparison-with-groth16)).
- Static memory sits at 88% of pallet-revive's limit.
- The largest trace tested (2^20) uses 73% of the ref_time cap for an Ethereum-style
  transaction.

## Results

Statement: Winterfell's `fib2` AIR over Goldilocks with a quadratic extension and FRI
folding factor 8. Blob: 332,587 bytes, sha256
`98df40b9865687f1a424415074de1d69e44a9bc78293ba64e3f81e587c4e19aa`. Two clean builds with
the pinned toolchain on aarch64-darwin produced this hash; other hosts have not been checked.

| trace | queries | blowup | grinding | conjectured bits (classical, ethSTARK Conj. 1) | proven bits (list / unique decoding) | calldata | PVM instructions | host calls | ref_time: local estimate | ref_time: live dry-run |
|---|---|---|---|---|---|---|---|---|---|---|
| 2^10 | 27 | 8 | 16 | 96 | 56 / 38 | 25,851 B | 2,553,737 | 863 | 2.045e11 | **2.046e11** |
| 2^10 | 20 | 32 | 16 | 115 | 64 / 35 | 25,895 B | 2,267,348 | 850 | 1.848e11 | **1.849e11** |
| 2^10 | 40 | 8 | 0 | 119 | 59 / 33 | 35,276 B | 3,634,839 | 1,188 | 2.895e11 | **2.897e11** |
| 2^10 | 28 | 16 | 16 | 127 | 70 / 41 | 30,821 B | 2,915,614 | 1,020 | 2.348e11 | **2.350e11** |
| 2^16 | 27 | 8 | 16 | 96 | 55 / 38 | 59,079 B | 5,284,160 | 1,848 | 4.257e11 | **4.260e11** |
| 2^20 | 27 | 8 | 16 | 96 | 54 / 38 | 86,031 B | 7,134,998 | 2,651 | 5.811e11 | **5.814e11** |
| 2^10, SHA3 in VM | 27 | 8 | 16 | 96 | 56 / 38 | 25,788 B | 7,951,012 | 4 | 5.307e11 | **5.307e11** |

* **Host calls** counts every host function call. In the keccak builds all but 4 are
  `hash_keccak_256`; the other 4 are call-data reads and the return.
* **Live dry-run** is measured. It calls `ReviveApi_instantiate` on Kusama Asset Hub (spec
  2,003,002, blocks #22,449,427 to #22,449,506, 2026-10-11) with the blob and proof as
  constructor data, and reports `weightConsumed` minus the same call with empty data.
* **Local estimate** comes from `host run`. It runs the blob in polkavm 0.33.1 (the
  runtime's version) with pallet-revive's settings, then applies the runtime's weights. It
  agrees with the dry-run to within 0.1%.
* **Overhead of a real `call` to a deployed contract** is computed from the weights, not
  measured: about 1e9 ref_time and about 344 KB proof_size (pallet-revive charges one proof
  byte per code byte).
* **Every row returns `0x01`.** Each of the following reverts with `0x00`:
  - a flipped proof bit
  - a changed public input (`result + 1`)
  - a changed proof-context byte
  - a truncated proof

### Security levels

* **Conjectured bits** are Winterfell's classical estimate under ethSTARK Conjecture 1.
  **Proven bits** are Winterfell's provable bounds (eprint 2024/1553), which are much lower.
* **Against a quantum adversary**, the query and grinding contributions roughly halve
  (Grover). Keccak-256's collision resistance drops from 128 to about 85 bits (BHT).
  - The 127-bit row therefore gives roughly 64 bits post-quantum.
  - The 96-bit rows give roughly 48 bits post-quantum.
* **None of these rows is PQ-adequate** at a 128-bit target. A ~100-bit post-quantum level
  needs about 200 classical query/grinding bits, for example blowup 16 with about 46 queries
  and 16 grinding bits. Keccak-256's ~85-bit quantum collision bound would then be the
  ceiling, so going higher needs a wider digest.
* The contract accepts any proof with ≥ 96 conjectured bits. That is a spike setting, not a
  recommendation.

### Comparison with Groth16

The baseline is the Groth16 verifier from [zkattest](https://github.com/msmrez/zkattest). It
is the author's own earlier project: a snarkjs/Solidity verifier compiled with resolc that
uses the BN254 precompiles. It is deployed on Polkadot Asset Hub at
`0xca1a3ad129e204d9af942a30f7f09282d20e28bf`. It is not deployed on Kusama.

| Runtime | Groth16 verify | STARK verify (2^10, 96-bit) | Ratio |
|---|---|---|---|
| Polkadot AH, spec 2,005,000: both measured by live dry-run (`scripts/groth16-dryrun.mjs`, `scripts/dryrun.mjs`) | 2.003e10 | 8.901e10 | **~4.4x** |
| Kusama AH, spec 2,003,002: Groth16 estimated from the BN254 precompile weights | ~2.9e10 | 2.046e11 (measured) | **~7x** |

* The Groth16 number is the `weightConsumed` delta between a `verifyProof` call and a call
  with an unknown selector.
* Groth16 call data is 356 bytes; STARK call data is 25.8 KB (about 70x).
* The Groth16 dry-run uses curve generators as the proof. They are valid points, so the full
  pairing check runs; it returns `false`, which costs the same as a valid proof.
* zkattest's toy-circuit verifier (`0x4fa8678f…e468`, 1 public input) measures 1.867e10 the
  same way, so the pairing check dominates.
* zkattest's README records about 3.93e9 ref_time. I could not reproduce that against the
  current runtime, and the figures here supersede it.
* Kusama Asset Hub has not yet enacted fellowship release v2.5.0. That release's Kusama
  weights cut the `hash_keccak_256` base cost from 39.4M to 1.05M, which would remove most of
  the 3.4e10 host-call share above. This is an estimate.

### Limits on Kusama Asset Hub (runtime v2.3.2, pallet-revive 0.18.0)

| Limit | Value | This spike |
|---|---|---|
| Code blob ([`limits.rs`][limits] `BLOB_BYTES`) | 1 MiB | 332,587 B (32%) |
| Static memory: blob + data + bss + stack (`BASELINE_MEMORY_LIMIT`) | 1.5 MiB | 1,387,355 B (88%) |
| Basic block size (`BASIC_BLOCK_SIZE`) | 1,000 instructions | 518 |
| Call data (`CALLDATA_BYTES`) | 128 KiB | 26–86 KB |
| Max ref_time per extrinsic, normal class (on chain) | 1.5999e12 | 2.0e11–5.8e11 |
| Max ref_time per Ethereum tx (`MaxEthExtrinsicWeight` = 0.5) | 7.999e11 | up to 5.8e11 (73%) |
| ref_time per PVM instruction (`instr` − `instr_empty_loop` weights) | 66,743 | |
| `hash_keccak_256` host call | 39,401,832 + 3,010/byte | |

[limits]: https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/revive/src/limits.rs

### Findings

1. **Winterfell's Blake3 cannot be deployed.** The `blake3` crate's portable
   `compress_in_place` compiles to a single basic block of 1,329 instructions. pallet-revive
   rejects any blob with a basic block over 1,000 instructions, including dead code.
2. **Winterfell's FRI code also trips that limit.** The cubic-extension monomorphization of
   `FriVerifier::verify_generic::<8>` is a 1,337-instruction block. The workaround is to build
   with `-Cllvm-args=-unroll-threshold=100`, which `build-contract.sh` does.
3. **Hashing dominates in-VM cost.** With SHA3 inside the VM, Keccak-f is about 63% of
   executed instructions. The `hash_keccak_256` host function cuts total ref_time by 2.6x
   (`air/src/keccak.rs`).
4. **Where the remaining keccak-build cost goes** (approximate):

   | Component | Share of instructions |
   |---|---|
   | FRI arithmetic | 38% |
   | `memcpy`/`memmove` | 14% |
   | Merkle path handling | 10% |
   | byte-by-byte `Vec<u8>` deserialization (`winter-utils`) | 8% |
   | remainder-polynomial checks | 8% |

   Method: one step-traced run in polkavm's interpreter. Each executed instruction was
   attributed to the innermost function in the blob's debug info, so inlined code counts
   toward its callee. The profiler is not part of this repo.
5. **Memory is the tight budget.** The heap (768 KiB `.bss`), the stack (256 KiB) and the
   blob share 1.5 MiB.
   - A 2^20 trace allocates about 800 KB cumulatively: about 713 KB of heap allocations plus
     the 86 KB call-data copy.
   - Peak live heap is only about 133 KB (measured natively), so the bump allocator
     reclaiming LIFO frees is enough.

## Layout

```
air/        the AIR (statement), proof options and the Keccak hasher, shared by both sides (no_std)
contract/   the PolkaVM contract: call data -> winter_verifier::verify -> return / revert
host/       prove, link (ELF -> .polkavm), check (pallet-revive upload rules), run (local PolkaVM)
scripts/    bench.sh (the table above), dryrun.mjs and groth16-dryrun.mjs (live dry-runs)
```

**Contract ABI (spike-level).** Call data is `result: u64 LE || winterfell proof bytes`. The
contract returns `0x01` on success and reverts with `0x00` on failure. Known gaps, which M1
will fix:
- Trailing bytes after the proof are accepted. M1 will reject them.
- A `result` ≥ p is reduced mod p. M1 will require a canonical u64.
- The trace length is chosen by the prover. M1 will pin it or include n in the public
  inputs.

## Reproduce

You need Rust 1.99.0 with `rust-src`; `rust-toolchain.toml` installs it. The dry-runs also
need Node.js ≥ 20.

```sh
./build-contract.sh                              # -> target/stark-verifier.polkavm, then runs the upload checks
./target/release/host prove 10 27 8 16 p.bin     # log2(steps) queries blowup grinding
./target/release/host run target/stark-verifier.polkavm p.bin

# each of these must revert with 0x00
python3 - <<'EOF'
d = bytearray(open('p.bin', 'rb').read())
r = int.from_bytes(d[:8], 'little')
for name, i, x in [('bit', len(d) // 2, 64), ('ctx', 12, 1)]:
    t = bytearray(d); t[i] ^= x; open(f'bad-{name}.bin', 'wb').write(t)
t = bytearray(d); t[:8] = (r + 1).to_bytes(8, 'little'); open('bad-result.bin', 'wb').write(t)
EOF
for f in bad-bit bad-ctx bad-result; do ./target/release/host run target/stark-verifier.polkavm $f.bin | tail -1; done

# the whole table, including the SHA3 row; set ORIGIN to any account that is mapped in
# pallet-revive and holds >= ~0.2 KSM to add the live dry-runs (nothing is signed)
(cd scripts && npm install)
ORIGIN=<your mapped account> ./scripts/bench.sh

# Groth16 baseline on Polkadot Asset Hub (origin: a mapped account there)
(cd scripts && RPC=wss://polkadot-asset-hub-rpc.polkadot.io node groth16-dryrun.mjs <origin>)
```

## Not done yet

* No real transaction has been submitted. The dry-runs execute the live runtimes but are not
  included in a block.
* There is no Solidity-compatible ABI, and the statement is a toy (Fibonacci).
* Proofs larger than 128 KiB need chunked upload. That case is not reached here.

## License

Dual-licensed under MIT ([LICENSE-MIT](LICENSE-MIT)) or Apache-2.0
([LICENSE-APACHE](LICENSE-APACHE)). `contract/riscv64emac-unknown-none-polkavm.json` is
copied from polkavm-linker 0.33.0 (MIT/Apache-2.0).
