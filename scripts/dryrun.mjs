// Dry-runs the verifier against the live Kusama Asset Hub runtime, without deploying anything.
//
//   node dryrun.mjs <blob.polkavm> <calldata.bin> <origin>
//
// Uses ReviveApi_instantiate (a read-only runtime API call): it uploads the blob in a throw-away
// overlay and runs its constructor, which verifies the proof passed as constructor data. Nothing
// is signed or submitted. <origin> is any account that is mapped in pallet-revive and holds
// ~0.1 KSM, because the dry-run checks that the code upload deposit could be paid. The run is
// repeated with empty data to separate the verification cost from the constructor overhead.
import { readFileSync } from 'node:fs';
import { ApiPromise, WsProvider } from '@polkadot/api';

const [blobPath, calldataPath, origin] = process.argv.slice(2);
const rpc = process.env.RPC ?? 'wss://kusama-asset-hub-rpc.polkadot.io';
const api = await ApiPromise.create({ provider: new WsProvider(rpc), noInitWarn: true });
// Hex strings, so that polkadot.js does not mistake the raw bytes for SCALE-encoded `Bytes`.
const code = '0x' + readFileSync(blobPath).toString('hex');
const calldata = '0x' + readFileSync(calldataPath).toString('hex');

const dryRun = async (data) => {
  const r = await api.call.reviveApi.instantiate(origin, 0, null, null, { Upload: code }, data, null);
  return r.toJSON();
};

const v = api.runtimeVersion;
const head = await api.rpc.chain.getHeader();
console.log(`${v.specName} spec ${v.specVersion} at block #${head.number}`);
const empty = await dryRun('0x');
const full = await dryRun(calldata);
for (const [name, r] of [['empty constructor', empty], ['verify proof', full]]) {
  console.log(name, JSON.stringify({ weightConsumed: r.weightConsumed, weightRequired: r.weightRequired, result: r.result }));
}
const d = (k) => full.weightConsumed[k] - empty.weightConsumed[k];
const out = full.result.ok?.result;
console.log(`verification only: ref_time ${d('refTime').toExponential(3)}, returned ${out?.data} (revert: ${out ? (out.flags.bits & 1) === 1 : 'n/a'})`);
await api.disconnect();
