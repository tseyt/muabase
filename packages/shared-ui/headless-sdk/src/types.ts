export type TerminologyLocale = "institutional" | "muamalat";

export interface MuamalatConfig {
  endpoint: string; // psinode URL (e.g. http://localhost:8080 or https://node.muamalatos.org)
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
  minAdalahScore: number;
  tangibleRatioBps: number; // >= 5100
  rofrWindowSec: number;
  custodianAccount: string;
  custodianFeeBps: number;
  charitySinkAccount: string;
}

export interface CreateAssetInput {
  title: string;
  dayRateUsd: number;
  replacementValueUsd: number;
  tangibleRatioBps?: number; // Defaults to 10000 (100%)
  rofrWindowSec?: number;    // Defaults to 86400 (24h)
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
