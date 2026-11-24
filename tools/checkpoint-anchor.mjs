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

const KECCAK_ROUNDS = 24;
const KECCAK_RC = [
  0x0000000000000001n, 0x0000000000008082n, 0x800000000000808an, 0x8000000080008000n,
  0x000000000000808bn, 0x0000000080000001n, 0x8000000080008081n, 0x8000000000008009n,
  0x000000000000008an, 0x0000000000000088n, 0x0000000080008009n, 0x000000008000000an,
  0x000000008000808bn, 0x800000000000008bn, 0x8000000000008089n, 0x8000000000008003n,
  0x8000000000008002n, 0x8000000000000080n, 0x000000000000800an, 0x800000008000000an,
  0x8000000080008081n, 0x8000000000008080n, 0x0000000080000001n, 0x8000000080008008n,
];
const KECCAK_ROT = [
  [0, 36, 3, 41, 18],
  [1, 44, 10, 45, 2],
  [62, 6, 43, 15, 61],
  [28, 55, 25, 21, 56],
  [27, 20, 39, 8, 14],
];
const MASK_64 = (1n << 64n) - 1n;

function rotl64(x, n) {
  const shift = BigInt(n);
  return shift === 0n ? x : ((x << shift) | (x >> (64n - shift))) & MASK_64;
}

function keccakF1600(state) {
  const b = new Array(25).fill(0n);
  const c = new Array(5).fill(0n);
  for (let round = 0; round < KECCAK_ROUNDS; round++) {
    for (let x = 0; x < 5; x++) {
      c[x] = state[x] ^ state[x + 5] ^ state[x + 10] ^ state[x + 15] ^ state[x + 20];
    }
    for (let x = 0; x < 5; x++) {
      const d = c[(x + 4) % 5] ^ rotl64(c[(x + 1) % 5], 1);
      for (let y = 0; y < 25; y += 5) state[x + y] ^= d;
    }
    for (let x = 0; x < 5; x++) {
      for (let y = 0; y < 5; y++) {
        b[y + ((2 * x + 3 * y) % 5) * 5] = rotl64(state[x + y * 5], KECCAK_ROT[x][y]);
      }
    }
    for (let y = 0; y < 25; y += 5) {
      for (let x = 0; x < 5; x++) {
        state[x + y] = b[x + y] ^ ((~b[((x + 1) % 5) + y] & MASK_64) & b[((x + 2) % 5) + y]);
      }
    }
    state[0] ^= KECCAK_RC[round];
  }
}

/** Zero-dependency Ethereum Keccak-256 (0x01 domain separator, not NIST SHA3-256). */
export function keccak256Hex(input) {
  const msg = Buffer.isBuffer(input) ? input : Buffer.from(String(input), 'utf8');
  const rate = 136; // 1088 bits
  const state = new Array(25).fill(0n);
  const rem = msg.length % rate;
  const padLen = rate - rem;
  const padded = Buffer.alloc(msg.length + padLen);
  msg.copy(padded);
  padded[msg.length] |= 0x01;
  padded[padded.length - 1] |= 0x80;

  for (let offset = 0; offset < padded.length; offset += rate) {
    for (let i = 0; i < rate / 8; i++) {
      let lane = 0n;
      for (let j = 0; j < 8; j++) {
        lane |= BigInt(padded[offset + i * 8 + j]) << BigInt(8 * j);
      }
      state[i] ^= lane;
    }
    keccakF1600(state);
  }

  const out = Buffer.alloc(32);
  for (let i = 0; i < 4; i++) {
    let lane = state[i];
    for (let j = 0; j < 8; j++) {
      out[i * 8 + j] = Number((lane >> BigInt(8 * j)) & 0xffn);
    }
  }
  return out.toString('hex');
}

export function evmFunctionSelector(signature) {
  return keccak256Hex(signature).slice(0, 8);
}

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
  const selector = evmFunctionSelector('anchorCheckpoint(uint64,bytes32,bytes32)');
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
  if (!proof?.anchor?.blockId) {
    throw new Error('Node /native/proof/state response missing anchor object');
  }
  return buildCheckpointAnchorBundle({
    blockNum: proof.anchor.blockNum,
    blockId: proof.anchor.blockId,
    producer: proof.anchor.producer,
    stateRoot: proof.anchor.stateRoot,
    headerSignature: proof.anchor.headerSignature || '',
    targetChain,
  });
}

const USAGE =
  'Usage:\n' +
  '  node tools/checkpoint-anchor.mjs --api http://127.0.0.1:8080 [--target-chain bitcoin_op_return] [--key-utf8 status]\n' +
  '  node tools/checkpoint-anchor.mjs --from-proof <proof.json> [--target-chain bitcoin_op_return]\n' +
  '  node tools/checkpoint-anchor.mjs --verify <anchor.json>';

function parseCliArgs(args) {
  const opts = {
    verifyPath: null,
    proofPath: null,
    apiUrl: null,
    targetChain: 'bitcoin_op_return',
    keyUtf8: 'status',
  };
  const requireValue = (flag, idx) => {
    if (idx + 1 >= args.length || args[idx + 1].startsWith('--')) {
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
    } else if (a === '--key-utf8') {
      opts.keyUtf8 = requireValue(a, i++);
    } else {
      throw new Error(`Unknown argument: ${a}\n${USAGE}`);
    }
  }
  return opts;
}

async function runCli() {
  const args = process.argv.slice(2);
  if (args.includes('--help') || args.includes('-h')) {
    console.log(USAGE);
    process.exit(0);
  }
  if (args.length === 0) {
    console.error(USAGE);
    process.exit(2);
  }

  const { verifyPath, proofPath, apiUrl, targetChain, keyUtf8 } = parseCliArgs(args);

  if (verifyPath) {
    const bundle = JSON.parse(readFileSync(verifyPath, 'utf8'));
    const res = verifyCheckpointAnchorBundle(bundle);
    console.log(JSON.stringify(res, null, 2));
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
    const bundle = await fetchFromNode(apiUrl, { keyUtf8, targetChain });
    console.log(JSON.stringify(bundle, null, 2));
    return;
  }

  throw new Error(`Specify --api, --from-proof, or --verify\n${USAGE}`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  runCli().catch((err) => {
    console.error(err instanceof Error ? err.message : String(err));
    process.exit(1);
  });
}
