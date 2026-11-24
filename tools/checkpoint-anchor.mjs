#!/usr/bin/env node
// Lightweight federated checkpoint-anchoring tool for Muabase.
// Exports a signed block header + dwal_merkle state root into deterministic external anchor
// payloads (Bitcoin OP_RETURN, EVM calldata, transparency log entry) and emits the exact
// `psibase push` action array for `producers::recordExternalAnchor`.

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const CHECKPOINT_ANCHOR_VERSION = 'muabase.checkpoint_anchor.v1';

const OP_RETURN_MAGIC = 'MUA1';
const ANCHOR_DIGEST_DOMAIN = 'muabase.checkpoint.v1:';
const MAX_BITCOIN_OP_RETURN_BYTES = 80;
const MAX_UINT32 = 0xffffffff;

function cleanHex32(hex, label) {
  if (typeof hex !== 'string') {
    throw new Error(`${label} must be a 32-byte (64-hex-char) digest`);
  }
  const clean = hex.replace(/^0x/i, '').toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(clean)) {
    throw new Error(`${label} must be a 32-byte (64-hex-char) digest`);
  }
  return clean;
}

/**
 * Encode the 56-byte Bitcoin OP_RETURN commitment (well under the 80-byte standardness limit):
 * "MUA1" (4B) || blockNum (4B BE) || stateRoot (32B) || blockId[0..16] (16B)
 */
export function encodeBitcoinOpReturn(blockNum, cleanStateRoot, cleanBlockId) {
  const magic = Buffer.from(OP_RETURN_MAGIC, 'ascii');
  const heightBuf = Buffer.alloc(4);
  heightBuf.writeUInt32BE(blockNum, 0);
  const rootBuf = Buffer.from(cleanStateRoot, 'hex');
  const blockPrefixBuf = Buffer.from(cleanBlockId.slice(0, 32), 'hex');
  return Buffer.concat([magic, heightBuf, rootBuf, blockPrefixBuf]);
}

/**
 * Encode deterministic EVM ABI calldata for
 * `anchorCheckpoint(uint64 blockNum, bytes32 stateRoot, bytes32 blockId)`.
 */
export function encodeEvmAnchorCalldata(blockNum, cleanStateRoot, cleanBlockId) {
  const selector = createHash('sha256')
    .update('anchorCheckpoint(uint64,bytes32,bytes32)')
    .digest('hex')
    .slice(0, 8);
  const evmHeightWord = BigInt(blockNum).toString(16).padStart(64, '0');
  return `0x${selector}${evmHeightWord}${cleanStateRoot}${cleanBlockId}`;
}

export function buildRecordExternalAnchorAction({ targetChain, blockNum, anchorDigest, cleanStateRoot }) {
  return [
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
  if (!Number.isInteger(blockNum) || blockNum < 0 || blockNum > MAX_UINT32) {
    throw new Error(`blockNum must be an integer in [0, ${MAX_UINT32}]`);
  }
  if (typeof targetChain !== 'string' || targetChain.trim().length === 0) {
    throw new Error('targetChain must be a non-empty string');
  }
  const cleanBlockId = cleanHex32(blockId, 'blockId');
  const cleanStateRoot = cleanHex32(stateRoot, 'stateRoot');

  const opReturnPayload = encodeBitcoinOpReturn(blockNum, cleanStateRoot, cleanBlockId);
  const opReturnHex = opReturnPayload.toString('hex');

  // Deterministic anchor commitment digest (also usable as transparency log leaf / simulated txHash)
  const anchorDigest = createHash('sha256')
    .update(Buffer.from(ANCHOR_DIGEST_DOMAIN, 'utf8'))
    .update(opReturnPayload)
    .digest('hex');

  const evmCalldata = encodeEvmAnchorCalldata(blockNum, cleanStateRoot, cleanBlockId);
  const recordExternalAnchorAction = buildRecordExternalAnchorAction({
    targetChain,
    blockNum,
    anchorDigest,
    cleanStateRoot,
  });

  return {
    version: CHECKPOINT_ANCHOR_VERSION,
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

function actionsMatch(actual, expected) {
  if (!Array.isArray(actual) || actual.length !== expected.length) return false;
  for (let i = 0; i < expected.length; i++) {
    const a = actual[i];
    const e = expected[i];
    if (!a || typeof a !== 'object' || !a.data || typeof a.data !== 'object') return false;
    if (
      a.sender !== e.sender ||
      a.service !== e.service ||
      a.method !== e.method ||
      a.data.chain_type !== e.data.chain_type ||
      a.data.block_height !== e.data.block_height ||
      a.data.tx_hash !== e.data.tx_hash ||
      a.data.state_root !== e.data.state_root
    ) {
      return false;
    }
  }
  return true;
}

export function verifyCheckpointAnchorBundle(bundle) {
  try {
    if (!bundle || typeof bundle !== 'object') {
      return { valid: false, reason: 'Checkpoint anchor bundle must be an object' };
    }
    if (bundle.version !== CHECKPOINT_ANCHOR_VERSION) {
      return { valid: false, reason: `Unsupported checkpoint anchor version: ${bundle.version}` };
    }
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
      recomputed.bitcoinOpReturnBytes === bundle.bitcoinOpReturnBytes &&
      recomputed.anchorDigest === bundle.anchorDigest &&
      recomputed.evmCalldata === bundle.evmCalldata &&
      bundle.bitcoinOpReturnBytes <= MAX_BITCOIN_OP_RETURN_BYTES &&
      actionsMatch(bundle.recordExternalAnchorAction, recomputed.recordExternalAnchorAction);
    return ok
      ? { valid: true, blockNum: bundle.blockNum, anchorDigest: bundle.anchorDigest }
      : { valid: false, reason: 'Checkpoint anchor payload mismatch' };
  } catch (err) {
    return { valid: false, reason: err instanceof Error ? err.message : String(err) };
  }
}

async function fetchFromNode(apiUrl, { keyUtf8 = 'status', targetChain = 'bitcoin_op_return' } = {}) {
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
  if (!proof?.anchor) {
    throw new Error('Node /native/proof/state response missing anchor object');
  }
  return buildCheckpointAnchorBundle({
    blockNum: proof.anchor.blockNum,
    blockId: proof.anchor.blockId || '0'.repeat(64),
    producer: proof.anchor.producer,
    stateRoot: proof.anchor.stateRoot,
    headerSignature: proof.anchor.headerSignature || '',
    targetChain,
  });
}

function parseCliArgs(args) {
  const opts = {
    verifyPath: null,
    proofPath: null,
    apiUrl: null,
    targetChain: 'bitcoin_op_return',
  };
  const requireValue = (flag, idx) => {
    if (idx + 1 >= args.length) {
      throw new Error(`Missing value for ${flag}`);
    }
    return args[idx + 1];
  };

  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === '--verify') {
      opts.verifyPath = requireValue(a, i++);
    } else if (a === '--from-proof') {
      opts.proofPath = requireValue(a, i++);
    } else if (a === '--api') {
      opts.apiUrl = requireValue(a, i++);
    } else if (a === '--target-chain') {
      opts.targetChain = requireValue(a, i++);
    } else {
      throw new Error(`Unknown argument: ${a}`);
    }
  }
  return opts;
}

async function runCli() {
  const args = process.argv.slice(2);
  if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
    console.error(
      'Usage:\n' +
        '  node tools/checkpoint-anchor.mjs --api http://127.0.0.1:8080 [--target-chain bitcoin_op_return]\n' +
        '  node tools/checkpoint-anchor.mjs --from-proof <proof.json> [--target-chain bitcoin_op_return]\n' +
        '  node tools/checkpoint-anchor.mjs --verify <anchor.json>'
    );
    process.exit(2);
  }

  const { verifyPath, proofPath, apiUrl, targetChain } = parseCliArgs(args);

  if (verifyPath) {
    const bundle = JSON.parse(readFileSync(verifyPath, 'utf8'));
    const res = verifyCheckpointAnchorBundle(bundle);
    console.log(JSON.stringify(res));
    process.exit(res.valid ? 0 : 1);
  }

  if (proofPath) {
    const proof = JSON.parse(readFileSync(proofPath, 'utf8'));
    if (!proof?.anchor) {
      throw new Error('Proof JSON missing anchor object');
    }
    const bundle = buildCheckpointAnchorBundle({
      blockNum: proof.anchor.blockNum,
      blockId: proof.anchor.blockId,
      producer: proof.anchor.producer,
      stateRoot: proof.anchor.stateRoot,
      headerSignature: proof.anchor.headerSignature || '',
      targetChain,
    });
    console.log(JSON.stringify(bundle, null, 2));
    return;
  }

  if (apiUrl) {
    const bundle = await fetchFromNode(apiUrl, { targetChain });
    console.log(JSON.stringify(bundle, null, 2));
    return;
  }

  throw new Error('Specify --api, --from-proof, or --verify');
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  runCli().catch((err) => {
    console.error(err instanceof Error ? err.message : String(err));
    process.exit(1);
  });
}
