import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildCheckpointAnchorBundle,
  verifyCheckpointAnchorBundle,
} from './checkpoint-anchor.mjs';

test('GIVEN a signed block header and dwal_merkle root WHEN building a checkpoint anchor THEN OP_RETURN stays <= 80 bytes, verifies cleanly, and rejects tampering', () => {
  const bundle = buildCheckpointAnchorBundle({
    chainId: 'muabase-corridor-1',
    blockNum: 3600,
    blockId: '00000e10' + 'a'.repeat(56),
    producer: 'qirim-ist-1',
    stateRoot: 'b'.repeat(64),
    targetChain: 'bitcoin_op_return',
  });

  assert.equal(bundle.bitcoinOpReturnBytes, 56);
  assert.ok(bundle.bitcoinOpReturnHex.startsWith('4d554131')); // "MUA1"
  assert.equal(bundle.recordExternalAnchorAction[0].service, 'producers');
  assert.equal(bundle.recordExternalAnchorAction[0].method, 'recordExternalAnchor');
  assert.equal(verifyCheckpointAnchorBundle(bundle).valid, true);

  const tampered = structuredClone(bundle);
  tampered.stateRoot = 'c'.repeat(64);
  assert.equal(verifyCheckpointAnchorBundle(tampered).valid, false);
});

test('GIVEN invalid blockNum, version, byte length, or tampered action fields WHEN building or verifying checkpoint anchors THEN rejected', () => {
  assert.throws(
    () =>
      buildCheckpointAnchorBundle({
        blockNum: 0x1_0000_0000,
        blockId: 'a'.repeat(64),
        stateRoot: 'b'.repeat(64),
      }),
    /blockNum must be an integer/,
  );

  const valid = buildCheckpointAnchorBundle({
    chainId: 'muabase-corridor-1',
    blockNum: 42,
    blockId: 'a'.repeat(64),
    producer: 'prod1',
    stateRoot: 'b'.repeat(64),
    targetChain: 'evm_sepolia',
  });

  const wrongVersion = structuredClone(valid);
  wrongVersion.version = 'muabase.checkpoint_anchor.v2';
  assert.equal(verifyCheckpointAnchorBundle(wrongVersion).valid, false);

  const tamperedAction = structuredClone(valid);
  tamperedAction.recordExternalAnchorAction[0].data.tx_hash = 'f'.repeat(64);
  assert.equal(verifyCheckpointAnchorBundle(tamperedAction).valid, false);

  const tamperedLength = structuredClone(valid);
  tamperedLength.bitcoinOpReturnBytes = 80;
  assert.equal(verifyCheckpointAnchorBundle(tamperedLength).valid, false);
});
