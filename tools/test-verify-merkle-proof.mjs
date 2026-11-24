import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  PROOF_VERSION,
  hashEntry,
  hashRootEnvelope,
  verifyAnchoredProof,
} from './verify-merkle-proof.mjs';

const toolsDir = dirname(fileURLToPath(import.meta.url));
const proofToolPath =
  process.env.MUABASE_PROOF_TOOL || resolve(toolsDir, '../build-native/muabase-proof-tool');

test('GIVEN live C++ dwal_merkle proofs WHEN verified offline in JS THEN leaf, terminal, subtree, single-entry, and non-inclusion proofs pass and tampering fails', () => {
  assert.ok(existsSync(proofToolPath), `Expected built C++ binary at ${proofToolPath}`);
  const stdout = execFileSync(proofToolPath, ['--emit-vectors'], { encoding: 'utf8' });
  const vectors = JSON.parse(stdout);

  for (const name of [
    'leafInclusion',
    'terminalInclusion',
    'subtreeInclusion',
    'nonInclusion',
    'singleEntryInclusion',
  ]) {
    const res = verifyAnchoredProof(vectors[name]);
    assert.equal(res.valid, true, `${name} should verify offline: ${res.reason}`);
  }

  // Tampering value must fail
  const tamperedValue = structuredClone(vectors.leafInclusion);
  tamperedValue.valueHex += '00';
  assert.equal(verifyAnchoredProof(tamperedValue).valid, false);

  // Tampering anchored stateRoot must fail
  const tamperedRoot = structuredClone(vectors.subtreeInclusion);
  tamperedRoot.anchor.stateRoot = 'f'.repeat(64);
  assert.equal(verifyAnchoredProof(tamperedRoot).valid, false);

  // Swapping non-inclusion target key onto a present key must fail
  const forgedAbsent = structuredClone(vectors.nonInclusion);
  forgedAbsent.proof.targetKeyHex = Buffer.from('receipt/0002', 'utf8').toString('hex');
  assert.equal(verifyAnchoredProof(forgedAbsent).valid, false);

  // Invalid branchByte (> 255) and invalid valueTag (> 1) must be rejected strictly
  const badBranch = structuredClone(vectors.leafInclusion);
  badBranch.proof.steps[0].siblings[0].branchByte = 256;
  assert.equal(verifyAnchoredProof(badBranch).valid, false);

  const badTag = structuredClone(vectors.leafInclusion);
  badTag.valueTag = 2;
  assert.equal(verifyAnchoredProof(badTag).valid, false);
});

test('GIVEN offline empty-tree, single-entry non-inclusion, and option-mismatch bundles WHEN verified THEN invariants hold without C++ binary', () => {
  const zeroRoot = '0'.repeat(64);
  const emptyAbsence = {
    version: PROOF_VERSION,
    anchor: { blockNum: 0, stateRoot: zeroRoot },
    proof: {
      isInclusion: false,
      emptyTree: true,
      singleEntry: false,
      targetKeyHex: Buffer.from('missing').toString('hex'),
      steps: [],
    },
  };
  assert.equal(verifyAnchoredProof(emptyAbsence).valid, true);
  assert.equal(
    verifyAnchoredProof(emptyAbsence, { expectedKeyBytes: Buffer.from('other') }).valid,
    false,
  );

  const witnessKey = Buffer.from('only-key', 'utf8');
  const witnessCore = hashEntry(witnessKey, 0, Buffer.from('val', 'utf8'));
  const singleRoot = hashRootEnvelope(witnessKey, witnessCore).toString('hex');
  const singleAbsence = {
    version: PROOF_VERSION,
    anchor: { blockNum: 1, stateRoot: singleRoot },
    proof: {
      isInclusion: false,
      emptyTree: false,
      singleEntry: true,
      targetKeyHex: Buffer.from('absent-key', 'utf8').toString('hex'),
      witnessKeyHex: witnessKey.toString('hex'),
      witnessCoreHash: witnessCore.toString('hex'),
      steps: [],
    },
  };
  assert.equal(verifyAnchoredProof(singleAbsence, { expectedRoot: singleRoot }).valid, true);
  assert.equal(verifyAnchoredProof(singleAbsence, { expectedRoot: 'a'.repeat(64) }).valid, false);

  // Claiming absence for the exact witness key must fail
  const forgedSelfAbsent = structuredClone(singleAbsence);
  forgedSelfAbsent.proof.targetKeyHex = witnessKey.toString('hex');
  assert.equal(verifyAnchoredProof(forgedSelfAbsent).valid, false);

  // Missing anchor/proof objects must fail gracefully
  assert.equal(verifyAnchoredProof({ version: PROOF_VERSION }).valid, false);
});
