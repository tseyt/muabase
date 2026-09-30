export type TerminologyLocale = "institutional" | "muamalat";

export interface MuabaseConfig {
  endpoint: string; // psinode URL (e.g. http://localhost:8080 or https://node.muabase.org)
  locale?: TerminologyLocale;
  account?: string;
  keyPair?: {
    publicKey: string;
    privateKey?: string;
  };
}

export interface RwaSeriesConfig {
  tokenId: number;
  legalWrapperHash: string; // SHA-256 of Series LLC / Trust agreement
  maxHolders: number;
  minTrustScore: number;    // Minimum trust score (0-10,000) to hold or rent
  minAdalahScore?: number;  // Backwards-compatible alias
  tangibleRatioBps: number; // >= 5100 (tangible_ratio)
  priorityWindowSec?: number; // Co-owner Right of First Refusal window in seconds
  rofrWindowSec: number;    // Backwards-compatible field
  custodianAccount: string;
  custodianFeeBps: number;
  charitySinkAccount: string;
}

export type AssetPoolConfig = RwaSeriesConfig;

export interface CreateAssetInput {
  title: string;
  dayRateUsd: number;
  replacementValueUsd: number;
  minTrustScore?: number;    // Defaults to 7000
  minAdalahScore?: number;   // Backwards-compatible alias
  tangibleRatioBps?: number; // Defaults to 10000 (100%)
  priorityWindowSec?: number;// Defaults to 86400 (24h)
  rofrWindowSec?: number;    // Backwards-compatible alias
  custodianAccount?: string;
  custodianFeeBps?: number;
  maxHolders?: number;
}

export interface UsufructLease {
  leaseId: string;
  lessor: string;
  lessee: string;
  tokenId: number;
  nftId?: number;
  durationSec: number;
  grossPayment: number;
  status: "Created" | "Active" | "ReturnedClean" | "Disputed" | "Completed";
  preConditionEvidenceHash: string;
  postConditionEvidenceHash?: string;
  isProgressiveBuyout: boolean;
  buyoutEquityUnits: number;
}

export interface CheckoutLeaseInput {
  assetId: number;
  durationDays: number;
  preConditionEvidenceHash: string;
  isProgressiveBuyout?: boolean;
  buyoutEquityUnits?: number;
  rail?: "stripe_connect" | "bitcoin_taproot" | "evm_safe" | "solana_squads";
}

export interface DamageClaimInput {
  leaseId: string;
  claimedAmountUsd: number;
  preConditionEvidenceHash: string;
  postConditionEvidenceHash: string;
  repairQuoteHash: string;
}
