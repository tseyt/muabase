#!/usr/bin/env node
// Lightweight federated checkpoint-anchoring tool for Muabase.
// Exports a signed block header + dwal_merkle state root into deterministic external anchor
// payloads (Bitcoin OP_RETURN, EVM calldata, transparency log entry) and emits the exact
// `psibase push` action array for `producers::recordExternalAnchor`.

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

function cleanHex32(hex, label) {
  const clean = String(hex).replace(/^0x/i, '').toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(clean)) {
    throw new Error(`${label} must be a 32-byte (64-hex-char) digest`);
  }
  return clean;
}

export function buildCheckpointAnchorBundle({
  chainId,
  blockNum,
  blockId,
  producer,
  stateRoot,
  headerSignature = '',
  targetChain = 'bitcoin_op_return',
}) {
  if (!Number.isInteger(blockNum) || blockNum < 0) {
    throw new Error('blockNum must be a non-negative integer');
  }
  const cleanBlockId = cleanHex32(blockId, 'blockId');
  const cleanStateRoot = cleanHex32(stateRoot, 'stateRoot');

  // 56-byte Bitcoin OP_RETURN commitment (well under 80-byte standardness limit):
  // "MUA1" (4B) || blockNum (4B BE) || stateRoot (32B) || blockId[0..16] (16B)
  const magic = Buffer.from('MUA1', 'ascii');
  const heightBuf = Buffer.alloc(4);
  heightBuf.writeUInt32BE(blockNum >>> 0, 0);
  const rootBuf = Buffer.from(cleanStateRoot, 'hex');
  const blockPrefixBuf = Buffer.from(cleanBlockId.slice(0, 32), 'hex');
  const opReturnPayload = Buffer.concat([magic, heightBuf, rootBuf, blockPrefixBuf]);
  const opReturnHex = opReturnPayload.toString('hex');

  // Deterministic anchor commitment digest (also usable as transparency log leaf / simulated txHash)
  const anchorDigest = createHash('sha256')
    .update(Buffer.from('muabase.checkpoint.v1:', 'utf8'))
    .update(opReturnPayload)
    .digest('hex');

  // EVM ABI calldata for anchorCheckpoint(uint64 blockNum, bytes32 stateRoot, bytes32 blockId)
  const selector = createHash('sha256')
    .update('anchorCheckpoint(uint64,bytes32,bytes32)')
    .digest('hex')
    .slice(0, 8);
  const evmHeightWord = BigInt(blockNum).toString(16).padStart(64, '0');
  const evmCalldata = `0x${selector}${evmHeightWord}${cleanStateRoot}${cleanBlockId}`;

  const recordExternalAnchorAction = [
    {
      sender: 'producers+3',
      service: 'producers',
      method: 'recordExternalAnchor',
      data: {
        chain_type: targetChain,
        block_height: blockNum,
        tx_hash: anchorDigest,
        state_root: cleanStateRoot,
      },
    },
  ];

  return {
    version: 'muabase.checkpoint_anchor.v1',
    chainId: chainId || '',
    blockNum,
    blockId: cleanBlockId,
    producer: producer || '',
    stateRoot: cleanStateRoot,
    headerSignature,
    targetChain,
    bitcoinOpReturnHex: opReturnHex,
    bitcoinOpReturnBytes: opReturnPayload.length,
    evmCalldata,
    anchorDigest,
    recordExternalAnchorAction,
  };
}

export function verifyCheckpointAnchorBundle(bundle) {
  try {
    const recomputed = buildCheckpointAnchorBundle({
      chainId: bundle.chainId,
      blockNum: bundle.blockNum,
      blockId: bundle.blockId,
      producer: bundle.producer,
      stateRoot: bundle.stateRoot,
      headerSignature: bundle.headerSignature,
      targetChain: bundle.targetChain,
    });
    const ok =
      recomputed.bitcoinOpReturnHex === bundle.bitcoinOpReturnHex &&
      recomputed.anchorDigest === bundle.anchorDigest &&
      recomputed.evmCalldata === bundle.evmCalldata &&
      bundle.bitcoinOpReturnBytes <= 80;
    return ok
      ? { valid: true, blockNum: bundle.blockNum, anchorDigest: bundle.anchorDigest }
      : { valid: false, reason: 'Checkpoint anchor payload mismatch' };
  } catch (err) {
    return { valid: false, reason: err instanceof Error ? err.message : String(err) };
  }
}

async function fetchFromNode(apiUrl, keyUtf8 = 'status') {
  const base = apiUrl.replace(/\/$/, '');
  const proofRes = await fetch(`${base}/native/proof/state`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ db: 'service', keyUtf8 }),
  });
  if (!proofRes.ok) {
    throw new Error(`Failed to fetch /native/proof/state [${proofRes.status}]: ${await proofRes.text()}`);
  }
  const proof = await proofRes.json();
  return buildCheckpointAnchorBundle({
    blockNum: proof.anchor.blockNum,
    blockId: proof.anchor.blockId || '0'.repeat(64),
    producer: proof.anchor.producer,
    stateRoot: proof.anchor.stateRoot,
    headerSignature: proof.anchor.headerSignature || '',
  });
}

async function runCli() {
  const args = process.argv.slice(2);
  if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
    console.error(
      'Usage:\n' +
        '  node tools/checkpoint-anchor.mjs --api http://127.0.0.1:8080\n' +
        '  node tools/checkpoint-anchor.mjs --from-proof <proof.json> [--target-chain bitcoin_op_return]\n' +
        '  node tools/checkpoint-anchor.mjs --verify <anchor.json>'
    );
    process.exit(2);
  }

  const getArg = (flag) => {
    const idx = args.indexOf(flag);
    return idx >= 0 ? args[idx + 1] : null;
  };

  const verifyPath = getArg('--verify');
  if (verifyPath) {
    const bundle = JSON.parse(readFileSync(verifyPath, 'utf8'));
    const res = verifyCheckpointAnchorBundle(bundle);
    console.log(JSON.stringify(res));
    process.exit(res.valid ? 0 : 1);
  }

  const proofPath = getArg('--from-proof');
  if (proofPath) {
    const proof = JSON.parse(readFileSync(proofPath, 'utf8'));
    const bundle = buildCheckpointAnchorBundle({
      blockNum: proof.anchor.blockNum,
      blockId: proof.anchor.blockId,
      producer: proof.anchor.producer,
      stateRoot: proof.anchor.stateRoot,
      headerSignature: proof.anchor.headerSignature || '',
      targetChain: getArg('--target-chain') || 'bitcoin_op_return',
    });
    console.log(JSON.stringify(bundle, null, 2));
    return;
  }

  const api = getArg('--api');
  if (api) {
    const bundle = await fetchFromNode(api);
    console.log(JSON.stringify(bundle, null, 2));
    return;
  }

  throw new Error('Specify --api, --from-proof, or --verify');
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  runCli().catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
