/**
 * Zero-dependency WebCrypto selective-disclosure helpers for Muabase client plugins.
 * Plaintext PII / B2B contract terms stay off-chain inside an AES-256-GCM ciphertext;
 * only the 32-byte salted SHA-256 commitment is recorded on-chain.
 */

export interface ViewKeyGrant {
  viewerId: string;
  wrappedKeyHex: string;
}

export interface SelectiveDisclosureEnvelope {
  version: "muabase.privacy.webcrypto.v1";
  commitmentHex: string;
  ivHex: string;
  ciphertextHex: string;
  grants: ViewKeyGrant[];
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function hexToBytes(hex: string): Uint8Array<ArrayBuffer> {
  const clean = hex.replace(/^0x/i, "");
  if (clean.length % 2 !== 0 || !/^[0-9a-fA-F]*$/.test(clean)) {
    throw new Error(`Invalid hex string: ${hex}`);
  }
  const out = new Uint8Array(new ArrayBuffer(clean.length / 2));
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function concatBytes(...chunks: Uint8Array[]): Uint8Array<ArrayBuffer> {
  const total = chunks.reduce((acc, c) => acc + c.length, 0);
  const out = new Uint8Array(new ArrayBuffer(total));
  let offset = 0;
  for (const c of chunks) {
    out.set(c, offset);
    offset += c.length;
  }
  return out;
}

async function sha256(bytes: Uint8Array): Promise<Uint8Array> {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  const digest = await crypto.subtle.digest("SHA-256", copy);
  return new Uint8Array(digest);
}

/**
 * Computes `0x` + SHA-256(salt || 0x1F || utf8(payload)) matching `muabase_institutional`.
 */
export async function computeSaltedCommitment(
  salt: Uint8Array,
  payload: string | Uint8Array,
): Promise<string> {
  if (salt.length !== 32) {
    throw new Error("Salt must be exactly 32 bytes");
  }
  const body = typeof payload === "string" ? new TextEncoder().encode(payload) : payload;
  const preimage = concatBytes(salt, new Uint8Array([0x1f]), body);
  return `0x${bytesToHex(await sha256(preimage))}`;
}

async function deriveWrapMask(viewerSecret: Uint8Array, iv: Uint8Array): Promise<Uint8Array> {
  const domain = new TextEncoder().encode("muabase.viewkey.wrap.v1");
  return sha256(concatBytes(domain, viewerSecret, iv));
}

export async function sealSelectiveDisclosureEnvelope(
  plaintext: string,
  viewers: Array<{ viewerId: string; secret: Uint8Array }>,
): Promise<SelectiveDisclosureEnvelope> {
  if (viewers.length === 0) {
    throw new Error("At least one selective-disclosure viewer is required");
  }
  const seenViewers = new Set<string>();
  for (const v of viewers) {
    if (!v.viewerId || v.viewerId.trim().length === 0) {
      throw new Error("Selective-disclosure viewerId must be non-empty");
    }
    if (seenViewers.has(v.viewerId)) {
      throw new Error(`Duplicate selective-disclosure viewerId '${v.viewerId}'`);
    }
    seenViewers.add(v.viewerId);
    if (v.secret.length !== 32) {
      throw new Error(`Viewer '${v.viewerId}' secret must be exactly 32 bytes`);
    }
  }

  const salt = crypto.getRandomValues(new Uint8Array(32));
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const rawKey = crypto.getRandomValues(new Uint8Array(32));

  const commitmentHex = await computeSaltedCommitment(salt, plaintext);
  const payloadBytes = new TextEncoder().encode(plaintext);
  const framed = concatBytes(salt, payloadBytes);

  const aesKey = await crypto.subtle.importKey("raw", rawKey, { name: "AES-GCM" }, false, ["encrypt"]);
  const aad = new TextEncoder().encode(commitmentHex);
  const cipherBuf = await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: aad }, aesKey, framed);

  const grants: ViewKeyGrant[] = [];
  for (const v of viewers) {
    const mask = await deriveWrapMask(v.secret, iv);
    const wrapped = new Uint8Array(32);
    for (let i = 0; i < 32; i++) wrapped[i] = rawKey[i] ^ mask[i];
    grants.push({ viewerId: v.viewerId, wrappedKeyHex: bytesToHex(wrapped) });
  }

  return {
    version: "muabase.privacy.webcrypto.v1",
    commitmentHex,
    ivHex: bytesToHex(iv),
    ciphertextHex: bytesToHex(new Uint8Array(cipherBuf)),
    grants,
  };
}

export async function openSelectiveDisclosureEnvelope(
  envelope: SelectiveDisclosureEnvelope,
  viewerId: string,
  viewerSecret: Uint8Array,
): Promise<string> {
  if (envelope.version !== "muabase.privacy.webcrypto.v1") {
    throw new Error("Unsupported envelope version");
  }
  if (viewerSecret.length !== 32) {
    throw new Error("Viewer secret must be exactly 32 bytes");
  }
  const grant = envelope.grants.find((g) => g.viewerId === viewerId);
  if (!grant) {
    throw new Error(`Viewer '${viewerId}' has no selective disclosure grant`);
  }

  const iv = hexToBytes(envelope.ivHex);
  if (iv.length !== 12) {
    throw new Error("Invalid 12-byte AES-GCM IV");
  }
  const wrapped = hexToBytes(grant.wrappedKeyHex);
  if (wrapped.length !== 32) {
    throw new Error("Invalid 32-byte wrapped key");
  }
  const mask = await deriveWrapMask(viewerSecret, iv);
  const rawKey = new Uint8Array(32);
  for (let i = 0; i < 32; i++) rawKey[i] = wrapped[i] ^ mask[i];

  const aesKey = await crypto.subtle.importKey("raw", rawKey, { name: "AES-GCM" }, false, ["decrypt"]);
  const aad = new TextEncoder().encode(envelope.commitmentHex);
  const cipherBytes = hexToBytes(envelope.ciphertextHex);
  const plainBuf = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: new Uint8Array(iv), additionalData: aad },
    aesKey,
    new Uint8Array(cipherBytes),
  );
  const framed = new Uint8Array(plainBuf);
  if (framed.length < 32) {
    throw new Error("Truncated decrypted payload");
  }
  const salt = framed.subarray(0, 32);
  const body = framed.subarray(32);
  const recomputed = await computeSaltedCommitment(salt, body);
  if (recomputed !== envelope.commitmentHex) {
    throw new Error("On-chain commitment mismatch after decryption");
  }
  return new TextDecoder().decode(body);
}
