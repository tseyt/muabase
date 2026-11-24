import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { verifyAnchoredProof } from './verify-merkle-proof.mjs';

const proofToolPath = resolve('build-native/muabase-proof-tool');

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
});
