import type { CreateZakatRemittanceSplitInput } from "./types.js";

const SHA256_HEX_RE = /^(?:0x)?([0-9a-fA-F]{64})$/;
const ZERO_SHA256_HEX = "0".repeat(64);
const ISO_CORRIDOR_RE = /^[A-Z]{2}$/;
const BPS_DENOMINATOR = 10_000;

export function assertPositiveInteger(value: number, fieldName: string): number {
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`${fieldName} must be a positive integer`);
  }
  return value;
}

export function assertNonNegativeInteger(value: number, fieldName: string): number {
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${fieldName} must be a non-negative integer`);
  }
  return value;
}

/**
 * Client-side Zero-PII guard: ensures `digestHex` is an exact, non-zero 32-byte SHA-256 hex string
 * (`0x`-prefixed or 64 raw hex chars) and never plaintext beneficiary PII.
 */
export function assertValidSha256Commitment(
  digestHex: string,
  fieldName = "recipientCommitmentHash",
): string {
  const match = SHA256_HEX_RE.exec(digestHex);
  if (!match) {
    throw new Error(
      `PII Guard Violation: ${fieldName} must be an exact 32-byte (64-hex-char) SHA-256 digest; plaintext PII is strictly forbidden`,
    );
  }
  const rawHex = match[1].toLowerCase();
  if (rawHex === ZERO_SHA256_HEX) {
    throw new Error(
      `PII Guard Violation: ${fieldName} cannot be an uninitialized all-zero 32-byte digest`,
    );
  }
  return `0x${rawHex}`;
}

/**
 * Client-side corridor code guard: enforces uppercase ISO-3166-1 alpha-2 bridge codes (e.g. `"TR"`, `"AE"`, `"UA"`, `"UZ"`).
 */
export function assertValidCorridorJurisdiction(corridor: string): string {
  if (!ISO_CORRIDOR_RE.test(corridor)) {
    throw new Error(
      `PII & Compliance Violation: corridorJurisdiction '${corridor}' must be a 2-letter uppercase ISO-3166-1 alpha-2 bridge code (e.g. TR, AE, UA, UZ)`,
    );
  }
  return corridor;
}

/**
 * Client-side *Tamlik* principal conservation guard:
 * Rejects any attempt to deduct protocol fees, gas, or FX spread from `zakatPrincipalAmount`
 * when `mode === "ZAKAT_TAMLIK_100"`.
 */
export function assertZakatTamlikSplitInvariant(input: CreateZakatRemittanceSplitInput): {
  mode: "COMMERCIAL_93_4_3" | "ZAKAT_TAMLIK_100";
  zakatPrincipalAmount: number;
  sadaqahTipAmount: number;
  protocolFeeDeduction: number;
} {
  const mode = input.mode ?? "ZAKAT_TAMLIK_100";
  const zakatPrincipalAmount = input.zakatPrincipalAmount;
  const sadaqahTipAmount = input.sadaqahTipAmount ?? 0;
  const protocolFeeDeduction = input.protocolFeeDeduction ?? 0;

  if (!Number.isInteger(zakatPrincipalAmount) || zakatPrincipalAmount <= 0) {
    throw new Error("zakatPrincipalAmount must be a positive integer in minor token units");
  }
  if (!Number.isInteger(sadaqahTipAmount) || sadaqahTipAmount < 0) {
    throw new Error("sadaqahTipAmount must be a non-negative integer in minor token units");
  }
  if (!Number.isInteger(protocolFeeDeduction) || protocolFeeDeduction < 0) {
    throw new Error("protocolFeeDeduction must be a non-negative integer in minor token units");
  }
  if (mode === "ZAKAT_TAMLIK_100" && protocolFeeDeduction > 0) {
    throw new Error(
      `Tamlik Violation: ZAKAT_TAMLIK_100 strictly forbids deducting any protocol fee, gas, or FX spread (${protocolFeeDeduction}) from zakatPrincipalAmount (${zakatPrincipalAmount})`,
    );
  }

  return { mode, zakatPrincipalAmount, sadaqahTipAmount, protocolFeeDeduction };
}

/**
 * Integer basis-point 93% / 4% / 3% commercial split matching `muabase_institutional::compute_commercial_split`.
 * Assigns any integer division truncation remainder to `ownerAmount` so exact conservation always holds.
 */
export function computeCommercialSplitCents(grossAmountCents: number): {
  ownerAmountCents: number;
  mutualShieldReserveCents: number;
  platformFeeCents: number;
} {
  assertPositiveInteger(grossAmountCents, "grossAmountCents");
  const platformFeeCents = Math.floor((grossAmountCents * 300) / BPS_DENOMINATOR);
  const mutualShieldReserveCents = Math.floor((grossAmountCents * 400) / BPS_DENOMINATOR);
  const ownerAmountCents = grossAmountCents - platformFeeCents - mutualShieldReserveCents;
  return { ownerAmountCents, mutualShieldReserveCents, platformFeeCents };
}
