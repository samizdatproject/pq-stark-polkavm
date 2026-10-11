# pq-stark-polkavm

A hash-based STARK verifier running as a native Rust contract on PolkaVM (pallet-revive),
targeting Kusama Asset Hub. This is a feasibility spike, not production code.

The verifier is [Winterfell](https://github.com/facebook/winterfell) 0.13.1 (`no_std`),
compiled to a `ReviveV1` PolkaVM blob. Hashing goes through pallet-revive's `hash_keccak_256`
host function. Security rests only on the hash function (Keccak-256) and FRI, with no
pairings and no trusted setup, so the scheme is plausibly post-quantum.

**Result: a Winterfell proof verifies inside PolkaVM in one transaction, well within Kusama
Asset Hub's limits, at about 50x the ref_time of a precompile-backed Groth16 verifier.**

## Results

Statement: Winterfell's `fib2` AIR over Goldilocks with a quadratic extension and FRI
folding factor 8. Blob: 332,587 bytes. All rows verify (`0x01`), and tampered proofs revert.

| trace | queries | blowup | grinding | conj. security | proof (calldata) | PVM instructions | keccak calls | ref_time: local estimate | ref_time: live dry-run |
|---|---|---|---|---|---|---|---|---|---|
| 2^10 | 27 | 8 | 16 | 96 bits | 25,851 B | 2,553,737 | 863 | 2.045e11 | **2.046e11** |
| 2^10 | 20 | 32 | 16 | 115 bits | 25,895 B | 2,267,348 | 850 | 1.848e11 | **1.849e11** |
| 2^10 | 40 | 8 | 0 | 119 bits | 35,276 B | 3,634,839 | 1,188 | 2.895e11 | **2.897e11** |
| 2^10 | 28 | 16 | 16 | 127 bits | 30,821 B | 2,915,614 | 1,020 | 2.348e11 | **2.350e11** |
| 2^16 | 27 | 8 | 16 | 96 bits | 59,079 B | 5,284,160 | 1,848 | 4.257e11 | **4.260e11** |
| 2^20 | 27 | 8 | 16 | 96 bits | 86,031 B | 7,134,998 | 2,651 | 5.811e11 | **5.814e11** |
| 2^10, SHA3 in VM | 27 | 8 | 16 | 96 bits | 25,788 B | 7,951,012 | 0 | 5.307e11 | **5.307e11** |

* **Live dry-run** is measured. It calls `ReviveApi_instantiate` on Kusama Asset Hub (spec
  2,003,002, blocks #22,447,755 to #22,447,860, 2026-10-11) with the blob and proof as
  constructor data, and reports `weightConsumed` minus the same call with empty data. It
  executes the real runtime and is not a submitted transaction.
* **Local estimate** comes from `host run`. It runs the blob in polkavm 0.33.1 (the version
  the runtime uses) with pallet-revive's settings, then applies the weights below. It agrees
  with the dry-run to within 0.1%.
* The weight of a real `call` to a deployed contract also includes call and code-load
  overhead. That is about 1e9 ref_time and about 344 KB proof_size (pallet-revive charges
  one proof byte per code byte). These figures are computed from the weights, not measured.

### Limits on Kusama Asset Hub (runtime v2.3.2, pallet-revive 0.18.0)

| Limit | Value | This spike |
|---|---|---|
| Code blob ([`limits.rs`][limits] `BLOB_BYTES`) | 1 MiB | 332,587 B |
| Static memory: blob + data + bss + stack (`BASELINE_MEMORY_LIMIT`) | 1.5 MiB | 1,387,355 B |
| Basic block size (`BASIC_BLOCK_SIZE`) | 1,000 instructions | 518 |
| Call data (`CALLDATA_BYTES`) | 128 KiB | 26–86 KB |
| Max ref_time per extrinsic, normal class (on chain) | 1.5999e12 | 2.0e11–5.8e11 |
| Max ref_time per Ethereum tx (`MaxEthExtrinsicWeight` = 0.5) | 7.999e11 | 2.0e11–5.8e11 |
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
3. **Hashing dominates in-VM cost.** With SHA3 inside the VM, Keccak-f is 63% of all
   instructions. Moving hashing to the `hash_keccak_256` host function cuts ref_time by 2.6x.
   `air/src/keccak.rs` is a 60-line Winterfell `Hasher` that does this.
4. **The remaining cost is mostly overhead that can be optimized**: FRI arithmetic 38%,
   `memcpy` 11%, Merkle path handling 10%, byte-by-byte `Vec<u8>` deserialization in
   `winter-utils` 8%, and remainder-polynomial checks 8%.
5. **Memory is the tight budget.** The heap (768 KiB `.bss`), the stack (256 KiB) and the
   blob share 1.5 MiB. A 2^20 trace needs about 800 KB of allocations. The bump allocator
   reclaims LIFO frees.
6. **Groth16 is far cheaper.** The [zkattest](https://github.com/msmrez/zkattest) Groth16
   verifier on Polkadot Asset Hub (`0xca1a3ad1…28bf`, Solidity via resolc, BN254
   precompiles) costs about 3.93e9 ref_time with a 256-byte proof. The STARK costs 52x more
   ref_time and has a 100x larger proof. In exchange it has no trusted setup and no pairings.

## Layout

```
air/        the AIR (statement), proof options and the Keccak hasher, shared by both sides (no_std)
contract/   the PolkaVM contract: call data -> winter_verifier::verify -> return / revert
host/       prove, link (ELF -> .polkavm), check (pallet-revive upload rules), run (local PolkaVM)
scripts/    bench.sh (the table above), dryrun.mjs (live Kusama Asset Hub dry-run)
```

Contract ABI (spike-level): call data is `result: u64 LE || winterfell proof bytes`. The
contract returns `0x01` on success and reverts with `0x00` on failure. It accepts any proof
with at least 96 bits of conjectured security.

## Reproduce

You need stable Rust 1.95 or later (tested with 1.99) with `rust-src` (`rustup component add rust-src`). The dry-run
also needs Node.js ≥ 20.

```sh
./build-contract.sh                              # -> target/stark-verifier.polkavm (+ upload checks via `host check`)
./target/release/host prove 10 27 8 16 p.bin     # log2(steps) queries blowup grinding
./target/release/host run target/stark-verifier.polkavm p.bin

# tamper with the proof: the contract reverts
python3 -c "d=bytearray(open('p.bin','rb').read()); d[len(d)//2]^=64; open('bad.bin','wb').write(d)"
./target/release/host run target/stark-verifier.polkavm bad.bin

# the whole table; ORIGIN is optional and enables the live dry-runs
(cd scripts && npm install)
ORIGIN=<mapped account with ~0.1 KSM> ./scripts/bench.sh
FEATURES=sha3-in-vm ./build-contract.sh          # SHA3-in-VM comparison build
```

## Not done yet

* No real transaction has been submitted. The dry-run executes the live runtime but is not
  included in a block.
* There is no Solidity-compatible ABI, and the statement is a toy (Fibonacci).
* Proofs larger than 128 KiB need chunked upload. That case is not reached here.

## License

MIT OR Apache-2.0. `contract/riscv64emac-unknown-none-polkavm.json` is copied from
polkavm-linker 0.33.0 (MIT/Apache-2.0).
