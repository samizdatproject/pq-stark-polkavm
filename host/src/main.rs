//! Off-chain tooling for the spike:
//!
//!   host prove <log2_steps> <queries> <blowup> <grinding> <out>   write contract call data
//!   host link  <elf> <out>                                         ELF -> PolkaVM blob (ReviveV1)
//!   host check <blob>                                              pallet-revive upload checks
//!   host run   <blob> <calldata>                                   execute like pallet-revive does
//!
//! `run` uses the same polkavm version (0.33.1, from the runtime's Cargo.lock), backend (interpreter), cost model (naive: one
//! gas per instruction), gas metering mode and page size as pallet-revive 0.18 on Kusama
//! Asset Hub, so the reported gas is the number pallet-revive multiplies by its per-
//! instruction weight. It is an estimate of ref_time, not an on-chain measurement.

use std::{env, fs, process::exit, time::Instant};

use air::{BaseElement, FibAir, FieldElement, ProofOptions, TRACE_WIDTH};
use polkavm::program::{
    EstimateInterpreterMemoryUsageArgs, ISA_ReviveV1, Instruction, InstructionSetKind,
};
use polkavm::{InterruptKind, ProgramBlob, Reg};
use sha3::Digest;
use winter_prover::{
    DefaultConstraintCommitment, DefaultConstraintEvaluator, DefaultTraceLde, Prover, Trace,
    TraceTable,
};

/// Kusama Asset Hub runtime v2.3.2 (spec 2_003_002), pallet_revive weights:
/// `instr(1) - instr(0) - (instr_empty_loop(1) - instr_empty_loop(0))` = 93_531 - 26_788.
/// This is what `ref_time_per_fuel()` in pallet-revive computes.
const REF_TIME_PER_GAS: u64 = 66_743;

/// Same runtime: ref_time charged per host function call, `(base, per byte)`.
const W_CALL_DATA_SIZE: (u64, u64) = (340_000, 0);
const W_CALL_DATA_COPY: (u64, u64) = (3_001_013, 143);
const W_KECCAK: (u64, u64) = (39_401_832, 3_010);
const W_RETURN: (u64, u64) = (819_578, 264);

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["prove", steps, q, b, g, out] => prove(n(steps), n(q), n(b), n(g) as u32, out),
        ["link", elf, out] => link(elf, out),
        ["check", blob] => check(&fs::read(blob).unwrap()),
        ["run", blob, calldata] => run(&fs::read(blob).unwrap(), &fs::read(calldata).unwrap()),
        _ => {
            eprintln!("usage: see the header of host/src/main.rs");
            exit(2)
        }
    }
}

fn n(s: &str) -> usize {
    s.parse().expect("expected a number")
}

// ---------------------------------------------------------------------------------------------
// Prover

struct FibProver(ProofOptions);

impl Prover for FibProver {
    type BaseField = BaseElement;
    type Air = FibAir;
    type Trace = TraceTable<BaseElement>;
    type HashFn = air::Hasher;
    type VC = air::VectorCommitment;
    type RandomCoin = air::RandomCoin;
    type TraceLde<E: FieldElement<BaseField = BaseElement>> =
        DefaultTraceLde<E, Self::HashFn, Self::VC>;
    type ConstraintCommitment<E: FieldElement<BaseField = BaseElement>> =
        DefaultConstraintCommitment<E, Self::HashFn, Self::VC>;
    type ConstraintEvaluator<'a, E: FieldElement<BaseField = BaseElement>> =
        DefaultConstraintEvaluator<'a, FibAir, E>;

    fn get_pub_inputs(&self, trace: &Self::Trace) -> BaseElement {
        trace.get(1, trace.length() - 1)
    }

    fn options(&self) -> &ProofOptions {
        &self.0
    }

    fn new_trace_lde<E: FieldElement<BaseField = BaseElement>>(
        &self,
        info: &winter_prover::TraceInfo,
        main: &winter_prover::matrix::ColMatrix<BaseElement>,
        domain: &winter_prover::StarkDomain<BaseElement>,
        partition: winter_air::PartitionOptions,
    ) -> (Self::TraceLde<E>, winter_prover::TracePolyTable<E>) {
        DefaultTraceLde::new(info, main, domain, partition)
    }

    fn new_evaluator<'a, E: FieldElement<BaseField = BaseElement>>(
        &self,
        air: &'a FibAir,
        aux: Option<winter_air::AuxRandElements<E>>,
        coeffs: winter_prover::ConstraintCompositionCoefficients<E>,
    ) -> Self::ConstraintEvaluator<'a, E> {
        DefaultConstraintEvaluator::new(air, aux, coeffs)
    }

    fn build_constraint_commitment<E: FieldElement<BaseField = BaseElement>>(
        &self,
        trace: winter_prover::CompositionPolyTrace<E>,
        num_columns: usize,
        domain: &winter_prover::StarkDomain<BaseElement>,
        partition: winter_air::PartitionOptions,
    ) -> (
        Self::ConstraintCommitment<E>,
        winter_prover::CompositionPoly<E>,
    ) {
        DefaultConstraintCommitment::new(trace, num_columns, domain, partition)
    }
}

fn prove(log2_steps: usize, queries: usize, blowup: usize, grinding: u32, out: &str) {
    let options = air::proof_options(queries, blowup, grinding);
    let mut trace = TraceTable::new(TRACE_WIDTH, 1 << log2_steps);
    trace.fill(
        |s| s.copy_from_slice(&[BaseElement::ONE; 2]),
        |_, s| {
            s[0] += s[1];
            s[1] += s[0];
        },
    );
    let result = trace.get(1, trace.length() - 1);

    let t = Instant::now();
    let proof = FibProver(options).prove(trace).expect("proving failed");
    let prove_ms = t.elapsed().as_millis();
    let security = proof.conjectured_security::<air::Hasher>().bits();

    // Sanity check with the native verifier before handing the proof to the contract.
    winter_verifier::verify::<FibAir, air::Hasher, air::RandomCoin, air::VectorCommitment>(
        proof.clone(),
        result,
        &winter_verifier::AcceptableOptions::MinConjecturedSecurity(air::MIN_SECURITY_BITS),
    )
    .expect("native verification failed");

    let mut calldata = result.as_int().to_le_bytes().to_vec();
    calldata.extend(proof.to_bytes());
    fs::write(out, &calldata).unwrap();
    println!(
        "steps=2^{log2_steps} queries={queries} blowup={blowup} grinding={grinding} \
         security={security}bits proof={}B calldata={}B prove={prove_ms}ms",
        calldata.len() - 8,
        calldata.len()
    );
}

// ---------------------------------------------------------------------------------------------
// Linking and upload checks

fn link(elf: &str, out: &str) {
    let mut config = polkavm_linker::Config::default();
    config.set_strip(true);
    config.set_optimize(true);
    let blob = polkavm_linker::program_from_elf(
        config,
        polkavm_linker::TargetInstructionSet::ReviveV1,
        &fs::read(elf).unwrap(),
    )
    .expect("link failed");
    fs::write(out, &blob).unwrap();
    println!("{out}: {} bytes", blob.len());
}

/// Re-implements `pallet_revive::limits::code::enforce` (pallet-revive 0.18.0) so a blob can be
/// checked against the live limits without a node.
fn check(code: &[u8]) {
    const BLOB_BYTES: u32 = 1024 * 1024;
    const BASIC_BLOCK_SIZE: u32 = 1000;
    const INTERPRETER_CACHE_BYTES: u32 = 1024 * 1024;
    const PURGABLE_MEMORY_LIMIT: u32 = INTERPRETER_CACHE_BYTES + 2 * 1024 * 1024;
    const BASELINE_MEMORY_LIMIT: u32 = BLOB_BYTES + 512 * 1024;

    let blob = ProgramBlob::parse(code.to_vec().into()).expect("not a PolkaVM blob");
    let (mut max_block, mut block, mut blocks, mut instrs) = (0u32, 0u32, 0u32, 0u32);
    for inst in blob.instructions_with_isa(ISA_ReviveV1) {
        block += 1;
        instrs += 1;
        if inst.kind.opcode().starts_new_basic_block() {
            max_block = max_block.max(block);
            block = 0;
            blocks += 1;
        }
        assert!(
            !matches!(inst.kind, Instruction::invalid | Instruction::sbrk(..)),
            "forbidden instruction"
        );
    }
    max_block = max_block.max(block);
    let mem = blob
        .estimate_interpreter_memory_usage(EstimateInterpreterMemoryUsageArgs::BoundedCache {
            max_cache_size_bytes: INTERPRETER_CACHE_BYTES,
            instruction_count: instrs,
            max_block_size: max_block,
            basic_block_count: blocks,
            page_size: 4096,
        })
        .unwrap();
    let imports: Vec<String> = blob
        .imports()
        .iter()
        .flatten()
        .map(|s| s.to_string())
        .collect();
    let ok = |b: bool| if b { "ok" } else { "FAIL" };

    println!(
        "blob size        {:>9} / {BLOB_BYTES} {}",
        code.len(),
        ok(code.len() as u32 <= BLOB_BYTES)
    );
    println!(
        "64-bit, ReviveV1 {}",
        ok(blob.is_64_bit() && blob.isa() == InstructionSetKind::ReviveV1)
    );
    println!(
        "max basic block  {max_block:>9} / {BASIC_BLOCK_SIZE} {}",
        ok(max_block <= BASIC_BLOCK_SIZE)
    );
    println!(
        "baseline memory  {:>9} / {BASELINE_MEMORY_LIMIT} {}",
        mem.baseline_ram_consumption,
        ok(mem.baseline_ram_consumption <= BASELINE_MEMORY_LIMIT)
    );
    println!(
        "purgeable memory {:>9} / {PURGABLE_MEMORY_LIMIT} {}",
        mem.purgeable_ram_consumption,
        ok(mem.purgeable_ram_consumption <= PURGABLE_MEMORY_LIMIT)
    );
    println!(
        "instructions     {instrs:>9}, imports: {}",
        imports.join(", ")
    );
}

// ---------------------------------------------------------------------------------------------
// Execution

fn run(code: &[u8], calldata: &[u8]) {
    check(code);

    let mut config = polkavm::Config::default();
    config.set_backend(Some(polkavm::BackendKind::Interpreter));
    let engine = polkavm::Engine::new(&config).unwrap();
    let mut module_config = polkavm::ModuleConfig::new();
    module_config.set_page_size(4096);
    module_config.set_gas_metering(Some(polkavm::GasMeteringKind::Sync));
    let module = polkavm::Module::new(&engine, &module_config, code.to_vec().into()).unwrap();
    let entry = module
        .exports()
        .find(|e| e.symbol().as_bytes() == b"call")
        .expect("no `call` export");

    const GAS_LIMIT: i64 = i64::MAX / 2;
    let mut vm = module.instantiate().unwrap();
    vm.set_gas(GAS_LIMIT);
    vm.prepare_call_untyped(entry.program_counter(), &[]);

    let t = Instant::now();
    let mut syscalls = 0u32;
    let mut syscall_ref_time = 0u64;
    let mut charge =
        |(base, per_byte): (u64, u64), bytes: u64| syscall_ref_time += base + per_byte * bytes;
    let output = loop {
        match vm.run().unwrap() {
            InterruptKind::Ecalli(idx) => {
                syscalls += 1;
                let name = module.imports().get(idx).unwrap();
                let (a0, a1, a2) = (vm.reg(Reg::A0), vm.reg(Reg::A1), vm.reg(Reg::A2));
                match name.as_bytes() {
                    b"call_data_size" => {
                        charge(W_CALL_DATA_SIZE, 0);
                        vm.set_reg(Reg::A0, calldata.len() as u64)
                    }
                    b"call_data_copy" => {
                        charge(W_CALL_DATA_COPY, a1);
                        // (out_ptr, out_len, offset): copy and zero-pad, as pallet-revive does.
                        let mut buf = vec![0u8; a1 as usize];
                        let src = calldata.get(a2 as usize..).unwrap_or(&[]);
                        let n = src.len().min(buf.len());
                        buf[..n].copy_from_slice(&src[..n]);
                        vm.write_memory(a0 as u32, &buf).unwrap();
                    }
                    b"hash_keccak_256" => {
                        // (input_ptr, input_len, out_ptr)
                        charge(W_KECCAK, a1);
                        let input = vm.read_memory(a0 as u32, a1 as u32).unwrap();
                        vm.write_memory(a2 as u32, &sha3::Keccak256::digest(&input))
                            .unwrap();
                    }
                    b"seal_return" => {
                        charge(W_RETURN, a2);
                        break (a0, vm.read_memory(a1 as u32, a2 as u32).unwrap());
                    }
                    other => panic!("unsupported syscall {}", String::from_utf8_lossy(other)),
                }
            }
            InterruptKind::Finished => break (0, vec![]),
            other => panic!("execution stopped: {other:?}"),
        }
    };
    let gas = (GAS_LIMIT - vm.gas()) as u64;
    let (flags, data) = output;
    let vm_ref_time = gas * REF_TIME_PER_GAS;
    println!(
        "result: flags={flags} data={data:?} | instructions {gas} | host calls {syscalls} \
         | est. ref_time: vm {:.3e} + host calls {:.3e} = {:.3e} | wall time {} ms",
        vm_ref_time as f64,
        syscall_ref_time as f64,
        (vm_ref_time + syscall_ref_time) as f64,
        t.elapsed().as_millis()
    );
}
