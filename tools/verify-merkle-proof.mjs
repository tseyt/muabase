#!/usr/bin/env node
// Zero-dependency offline verifier for Muabase `muabase.dwal_merkle.v1` JSON proofs.
// Cryptographically mirrors libraries/muabase_db/src/dwal_merkle.cpp (domain tags 0x01/0x02/0x03).

import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const ZERO_HASH = Buffer.alloc(32, 0);

export function hexToBytes(hex) {
  if (typeof hex !== 'string') throw new Error('Expected hex string');
  const clean = hex.startsWith('0x') || hex.startsWith('0X') ? hex.slice(2) : hex;
  if (clean.length % 2 !== 0 || !/^[0-9a-fA-F]*$/.test(clean)) {
    throw new Error(`Invalid hex string: ${hex}`);
  }
  return Buffer.from(clean, 'hex');
}

export function hexToChecksum(hex) {
  const buf = hexToBytes(hex);
  if (buf.length !== 32) throw new Error(`Expected 32-byte digest, got ${buf.length}`);
  return buf;
}

function u16le(n) {
  const b = Buffer.alloc(2);
  b.writeUInt16LE(n, 0);
  return b;
}

function u32le(n) {
  const b = Buffer.alloc(4);
  b.writeUInt32LE(n, 0);
  return b;
}

function bytesEqual(a, b) {
  return a.length === b.length && Buffer.compare(a, b) === 0;
}

function startsWith(buf, prefix) {
  return buf.length >= prefix.length && Buffer.compare(buf.subarray(0, prefix.length), prefix) === 0;
}

export function hashEntry(key, tag, value) {
  const h = createHash('sha256');
  h.update(Buffer.from([0x01]));
  h.update(u32le(key.length));
  if (key.length > 0) h.update(key);
  h.update(Buffer.from([tag & 0xff]));
  h.update(u32le(value.length));
  if (value.length > 0) h.update(value);
  return h.digest();
}

function hashSplitCore(hasTerminal, terminalHash, children) {
  const h = createHash('sha256');
  h.update(Buffer.from([0x02]));
  h.update(Buffer.from([hasTerminal ? 1 : 0]));
  if (hasTerminal) h.update(terminalHash);
  h.update(u16le(children.length));
  for (const c of children) {
    h.update(Buffer.from([c.isLeaf ? 1 : 0, c.branchByte & 0xff]));
    h.update(u16le(c.edgePrefix.length));
    if (c.edgePrefix.length > 0) h.update(c.edgePrefix);
    h.update(c.coreHash);
  }
  return h.digest();
}

function hashRootEnvelope(rootPrefix, coreHash) {
  const h = createHash('sha256');
  h.update(Buffer.from([0x03]));
  h.update(u16le(rootPrefix.length));
  if (rootPrefix.length > 0) h.update(rootPrefix);
  h.update(coreHash);
  return h.digest();
}

function strictlySortedSiblings(sibs) {
  for (let i = 1; i < sibs.length; i++) {
    if (sibs[i - 1].branchByte >= sibs[i].branchByte) return false;
  }
  return true;
}

function insertSiblingSorted(sibs, extra) {
  const out = [];
  let inserted = false;
  for (const s of sibs) {
    if (s.branchByte === extra.branchByte) return null;
    if (!inserted && extra.branchByte < s.branchByte) {
      out.push(extra);
      inserted = true;
    }
    out.push(s);
  }
  if (!inserted) out.push(extra);
  return out;
}

function decodeStep(step) {
  return {
    edgePrefix: hexToBytes(step.edgePrefixHex),
    hasTerminal: Boolean(step.hasTerminal),
    terminalHash: hexToChecksum(step.terminalHash),
    siblings: (step.siblings || []).map((s) => ({
      branchByte: Number(s.branchByte),
      isLeaf: Boolean(s.isLeaf),
      edgePrefix: hexToBytes(s.edgePrefixHex),
      coreHash: hexToChecksum(s.coreHash),
    })),
  };
}

export function verifyAnchoredProof(bundle, options = {}) {
  try {
    if (!bundle || bundle.version !== 'muabase.dwal_merkle.v1') {
      return { valid: false, reason: 'Unsupported or missing proof version' };
    }
    const expectedRoot = options.expectedRoot
      ? hexToChecksum(options.expectedRoot)
      : hexToChecksum(bundle.anchor.stateRoot);

    if (options.expectedRoot && !bytesEqual(expectedRoot, hexToChecksum(bundle.anchor.stateRoot))) {
      return { valid: false, reason: 'Anchor stateRoot does not match expectedRoot' };
    }

    const proof = bundle.proof;
    const targetKey = hexToBytes(proof.targetKeyHex);
    if (options.expectedKeyBytes && !bytesEqual(targetKey, Buffer.from(options.expectedKeyBytes))) {
      return { valid: false, reason: 'Proof targetKey does not match expected key' };
    }

    const valueTag = Number(bundle.valueTag ?? 0);
    const valueBytes = hexToBytes(bundle.valueHex ?? '');
    if (options.expectedValueBytes && !bytesEqual(valueBytes, Buffer.from(options.expectedValueBytes))) {
      return { valid: false, reason: 'Proof value does not match expected value' };
    }

    const steps = (proof.steps || []).map(decodeStep);

    if (proof.isInclusion) {
      if (proof.emptyTree || bytesEqual(expectedRoot, ZERO_HASH)) {
        return { valid: false, reason: 'Inclusion proof cannot match empty root' };
      }
      const entryHash = hashEntry(targetKey, valueTag, valueBytes);

      if (proof.singleEntry) {
        const rootPrefix = hexToBytes(proof.rootPrefixHex ?? '');
        if (steps.length !== 0 || !bytesEqual(rootPrefix, targetKey)) {
          return { valid: false, reason: 'Invalid single-entry inclusion metadata' };
        }
        const computed = hashRootEnvelope(targetKey, entryHash);
        return bytesEqual(computed, expectedRoot)
          ? { valid: true, mode: 'inclusion', blockNum: bundle.anchor.blockNum, stateRoot: bundle.anchor.stateRoot }
          : { valid: false, reason: 'Single-entry root mismatch' };
      }

      if (steps.length === 0) return { valid: false, reason: 'Missing proof steps' };

      const k = steps.length;
      const splitPositions = new Array(k).fill(0);
      let depth = 0;
      for (let i = 0; i < k; i++) {
        const step = steps[i];
        if (!strictlySortedSiblings(step.siblings)) return { valid: false, reason: 'Unsorted siblings' };
        if (targetKey.length < depth + step.edgePrefix.length || !startsWith(targetKey.subarray(depth), step.edgePrefix)) {
          return { valid: false, reason: 'Key prefix mismatch along proof path' };
        }
        splitPositions[i] = depth + step.edgePrefix.length;
        if (i + 1 < k) {
          if (targetKey.length <= splitPositions[i]) return { valid: false, reason: 'Key truncated before leaf' };
          depth = splitPositions[i] + 1;
        }
      }

      let curCore;
      const last = steps[k - 1];
      const lastSplit = splitPositions[k - 1];
      if (targetKey.length === lastSplit) {
        if (!last.hasTerminal || last.siblings.length === 0) {
          return { valid: false, reason: 'Invalid terminal split node' };
        }
        curCore = hashSplitCore(true, entryHash, last.siblings);
      } else {
        if (!last.hasTerminal && last.siblings.length === 0) {
          return { valid: false, reason: 'Degenerate split node' };
        }
        const b = targetKey[lastSplit];
        const rem = targetKey.subarray(lastSplit + 1);
        const merged = insertSiblingSorted(last.siblings, {
          branchByte: b,
          isLeaf: true,
          edgePrefix: rem,
          coreHash: entryHash,
        });
        if (!merged) return { valid: false, reason: 'Duplicate branch byte at leaf step' };
        curCore = hashSplitCore(last.hasTerminal, last.terminalHash, merged);
      }

      for (let idx = k - 1; idx > 0; idx--) {
        const i = idx - 1;
        const step = steps[i];
        const b = targetKey[splitPositions[i]];
        const merged = insertSiblingSorted(step.siblings, {
          branchByte: b,
          isLeaf: false,
          edgePrefix: steps[i + 1].edgePrefix,
          coreHash: curCore,
        });
        if (!merged || (!step.hasTerminal && merged.length < 2)) {
          return { valid: false, reason: 'Invalid internal split node fold' };
        }
        curCore = hashSplitCore(step.hasTerminal, step.terminalHash, merged);
      }

      const computed = hashRootEnvelope(steps[0].edgePrefix, curCore);
      return bytesEqual(computed, expectedRoot)
        ? { valid: true, mode: 'inclusion', blockNum: bundle.anchor.blockNum, stateRoot: bundle.anchor.stateRoot }
        : { valid: false, reason: 'Computed root does not match anchored stateRoot' };
    }

    // Non-inclusion proof verification
    if (proof.emptyTree) {
      const ok = steps.length === 0 && !proof.singleEntry && bytesEqual(expectedRoot, ZERO_HASH);
      return ok
        ? { valid: true, mode: 'non-inclusion', blockNum: bundle.anchor.blockNum, stateRoot: bundle.anchor.stateRoot }
        : { valid: false, reason: 'Empty-tree non-inclusion mismatch' };
    }
    if (bytesEqual(expectedRoot, ZERO_HASH)) return { valid: false, reason: 'Non-empty proof against zero root' };

    if (proof.singleEntry) {
      const witnessKey = hexToBytes(proof.witnessKeyHex ?? '');
      const witnessCoreHash = hexToChecksum(proof.witnessCoreHash);
      if (steps.length !== 0 || bytesEqual(witnessKey, targetKey)) {
        return { valid: false, reason: 'Invalid single-entry non-inclusion witness' };
      }
      const computed = hashRootEnvelope(witnessKey, witnessCoreHash);
      return bytesEqual(computed, expectedRoot)
        ? { valid: true, mode: 'non-inclusion', blockNum: bundle.anchor.blockNum, stateRoot: bundle.anchor.stateRoot }
        : { valid: false, reason: 'Single-entry witness root mismatch' };
    }

    if (steps.length === 0) return { valid: false, reason: 'Missing non-inclusion proof steps' };

    const k = steps.length;
    const splitPositions = new Array(k).fill(0);
    let depth = 0;
    for (let i = 0; i + 1 < k; i++) {
      const step = steps[i];
      if (!strictlySortedSiblings(step.siblings)) return { valid: false, reason: 'Unsorted siblings' };
      if (targetKey.length <= depth + step.edgePrefix.length || !startsWith(targetKey.subarray(depth), step.edgePrefix)) {
        return { valid: false, reason: 'Prefix mismatch in non-inclusion path' };
      }
      splitPositions[i] = depth + step.edgePrefix.length;
      depth = splitPositions[i] + 1;
    }

    const last = steps[k - 1];
    if (!strictlySortedSiblings(last.siblings)) return { valid: false, reason: 'Unsorted final siblings' };
    if ((last.hasTerminal && last.siblings.length === 0) || (!last.hasTerminal && last.siblings.length < 2)) {
      return { valid: false, reason: 'Invalid final split node cardinality' };
    }

    const splitPos = depth + last.edgePrefix.length;
    const pfxMatches = targetKey.length >= splitPos && startsWith(targetKey.subarray(depth), last.edgePrefix);
    let provenAbsent = false;
    if (!pfxMatches) {
      provenAbsent = true;
    } else if (targetKey.length === splitPos) {
      provenAbsent = !last.hasTerminal;
    } else {
      const b = targetKey[splitPos];
      const sib = last.siblings.find((s) => s.branchByte === b);
      if (!sib) {
        provenAbsent = true;
      } else if (sib.isLeaf) {
        const rem = targetKey.subarray(splitPos + 1);
        provenAbsent = !bytesEqual(rem, sib.edgePrefix);
      }
    }
    if (!provenAbsent) return { valid: false, reason: 'Final split node does not prove absence' };

    let curCore = hashSplitCore(last.hasTerminal, last.terminalHash, last.siblings);
    for (let idx = k - 1; idx > 0; idx--) {
      const i = idx - 1;
      const step = steps[i];
      const b = targetKey[splitPositions[i]];
      const merged = insertSiblingSorted(step.siblings, {
        branchByte: b,
        isLeaf: false,
        edgePrefix: steps[i + 1].edgePrefix,
        coreHash: curCore,
      });
      if (!merged || (!step.hasTerminal && merged.length < 2)) {
        return { valid: false, reason: 'Invalid non-inclusion internal fold' };
      }
      curCore = hashSplitCore(step.hasTerminal, step.terminalHash, merged);
    }

    const computed = hashRootEnvelope(steps[0].edgePrefix, curCore);
    return bytesEqual(computed, expectedRoot)
      ? { valid: true, mode: 'non-inclusion', blockNum: bundle.anchor.blockNum, stateRoot: bundle.anchor.stateRoot }
      : { valid: false, reason: 'Computed non-inclusion root mismatch' };
  } catch (err) {
    return { valid: false, reason: err instanceof Error ? err.message : String(err) };
  }
}

function runCli() {
  const args = process.argv.slice(2);
  if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
    console.error(
      'Usage: node tools/verify-merkle-proof.mjs <proof.json|--stdin> [--expected-root <hex>] [--expected-key-utf8 <str>] [--expected-value-utf8 <str>]'
    );
    process.exit(2);
  }

  let inputPath = null;
  const options = {};
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === '--stdin') inputPath = '--stdin';
    else if (a === '--expected-root') options.expectedRoot = args[++i];
    else if (a === '--expected-key-utf8') options.expectedKeyBytes = Buffer.from(args[++i], 'utf8');
    else if (a === '--expected-value-utf8') options.expectedValueBytes = Buffer.from(args[++i], 'utf8');
    else if (!inputPath) inputPath = a;
  }

  const raw = inputPath === '--stdin' ? readFileSync(0, 'utf8') : readFileSync(inputPath, 'utf8');
  const bundle = JSON.parse(raw);
  const res = verifyAnchoredProof(bundle, options);
  console.log(JSON.stringify(res));
  process.exit(res.valid ? 0 : 1);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  runCli();
}
