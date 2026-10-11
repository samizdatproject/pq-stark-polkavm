// Groth16 baseline: dry-runs the snarkjs/resolc Groth16 verifier of zkattest (the author's
// own earlier project, https://github.com/msmrez/zkattest), deployed on Polkadot Asset Hub,
// with ReviveApi_call. Reports weightConsumed minus the same call with an unknown selector.
//
//   RPC=wss://polkadot-asset-hub-rpc.polkadot.io node groth16-dryrun.mjs <mapped origin>
//
// The proof is made of curve generators: every point is valid, so the verifier runs all of its
// field checks, ecMul/ecAdd and the 4-pair pairing, and returns false. Groth16 verification
// does the same work whatever the proof, so this costs the same as a valid proof.
import { ApiPromise, WsProvider } from '@polkadot/api';
import { keccakAsU8a } from '@polkadot/util-crypto';
import { u8aToHex } from '@polkadot/util';

const VERIFIER = '0xca1a3ad129e204d9af942a30f7f09282d20e28bf';
const [origin] = process.argv.slice(2);
const api = await ApiPromise.create({ provider: new WsProvider(process.env.RPC), noInitWarn: true });

const word = (n) => BigInt(n).toString(16).padStart(64, '0');
const g1 = [1n, 2n];
// BN254 G2 generator in EIP-197 order: (x_imag, x_real), (y_imag, y_real).
const g2 = [
  [11559732032986387107991004021392285783925812861821192530917403151452391805634n,
   10857046999023057135944570762232829481370756359578518086990519993285655852781n],
  [4082367875863433681332203403145435568316851327593401208105741076214120093531n,
   8495653923123431417604973247489272438418190587263600148770280649306958101930n],
];
const selector = u8aToHex(keccakAsU8a('verifyProof(uint256[2],uint256[2][2],uint256[2],uint256[3])').slice(0, 4));
const args = [...g1, ...g2.flat(), ...g1, 1n, 2n, 3n].map(word).join('');
const call = async (data) => (await api.call.reviveApi.call(origin, VERIFIER, 0, null, null, data)).toJSON();

const v = api.runtimeVersion;
console.log(`${v.specName} spec ${v.specVersion} at block #${(await api.rpc.chain.getHeader()).number}`);
const base = await call('0xdeadbeef');
const full = await call(selector + args);
console.log('verify', JSON.stringify({ weightConsumed: full.weightConsumed, result: full.result }));
console.log(`verification only: ref_time ${(full.weightConsumed.refTime - base.weightConsumed.refTime).toExponential(3)}`);
await api.disconnect();
