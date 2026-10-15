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

export interface AssetPoolConfig {
  tokenId: number;
  legalWrapperHash: string; // SHA-256 of Series LLC / Trust agreement
  maxHolders: number;
  minTrustScore: number;    // Minimum trust score (0-10,000) to hold or rent
  minAdalahScore?: number;  // Backwards-compatible alias
  tangibleRatioBps: number; // >= 5100 (tangible_ratio)
  priorityWindowSec: number; // Co-owner Right of First Refusal window in seconds
  rofrWindowSec?: number;    // Backwards-compatible alias
  custodianAccount: string;
  custodianFeeBps: number;
  charitySinkAccount: string;
}

export type RwaSeriesConfig = AssetPoolConfig;

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

// Cooperative & Real Estate Infrastructure Types (MizanVest)
export interface CoopPoolInput {
  name: string;
  targetYieldMinBps: number; // e.g. 800 (8.00%)
  targetYieldMaxBps: number; // e.g. 1200 (12.00%)
  stabilizationReserveCapBps: number; // e.g. 500 (5.00% of portfolio)
  tangibleRatioBps?: number; // Defaults to 10000 (100% tangible)
  priorityWindowSec?: number; // Co-owner pre-emption (Haqq al-Shuf'ah)
  legalWrapperHash: string; // Master Series LLC / Trust deed hash
  custodianAccount?: string;
}

export interface RevenueReportInput {
  poolId: number;
  source: "workforce_housing_rent" | "commercial_solar_ppa";
  grossAmountCents: number;
  operatingExpensesCents: number;
  evidenceHash: string; // Utility statement or tenant ledger digest
}

export interface DistributionResult {
  distributionId: string;
  poolId: number;
  grossYieldBps: number;
  netYieldBps: number;
  effectivePayoutBps: number; // Clamped between 800 and 1200 bps
  reserveSurplusAddedCents: number;
  reserveDrawCents: number;
  reserveTotalBalanceCents: number;
}

// Ethical Forecasting & Ju'alah Knowledge Bounty Types (Irfan.Markets)
export interface CreateMarketInput {
  question: string;
  category: string;
  outcomes: string[];
  resolutionTimestamp: number;
  oracleAccount: string;
  minTrustScore?: number; // Required reliability score to participate
  descriptionHash: string;
}

export interface StakeAllocationInput {
  marketId: number;
  outcomeIndex: number;
  principalAmountUsd: number;
  rail?: "solana_marinade" | "native_usdc" | "evm_safe";
}

export interface MarketPosition {
  positionId: string;
  marketId: number;
  account: string;
  outcomeIndex: number;
  principalAmountUsd: number;
  prizeShares: number; // Time-weighted: Principal * Seconds
  allocationTimestamp: number;
  status: "Active" | "Claimed" | "Refunded";
}

export interface MarketResolutionInput {
  marketId: number;
  winningOutcomeIndex: number;
  evidenceHash: string;
  dualWitnessSignatures?: string[]; // Binary Bayyinah gate
}

export interface ClaimRewardResult {
  positionId: string;
  principalRefundUsd: number; // Always 100% of principal
  yieldPrizeRewardUsd: number; // Pro-rata share of accrued yield
  skillPointsEarned: number; // Soulbound trust_score increment
}

// Social Restaurant Tracking & Tastemaker Curation Types (Beli Competitor)
export interface RegisterRestaurantInput {
  name: string;
  cuisine: string;
  neighborhood: string;
  city: string;
  priceTier: 1 | 2 | 3 | 4;
  tangibleRatioBps?: number; // Defaults to 10000 (100% tangible physical dining)
  minTrustScore?: number; // Minimum reliability score to contribute official ratings
}

export interface ProofOfVisitInput {
  restaurantId: number;
  dinerAccount?: string;
  receiptDigestHash: string; // SHA-256 of bill / itemized POS check (Binary Bayyinah)
  visitTimestamp: number;
  overallRatingBps: number; // e.g. 8850 = 8.85 / 10.0
  dishesOrdered: string[];
  notes?: string;
  witnessAccounts?: string[]; // Optional 2-witness quorum (trust_score + domain_badge)
}

export interface PairwiseComparisonInput {
  winnerRestaurantId: number;
  loserRestaurantId: number;
  category?: "overall" | "food" | "vibe" | "service" | "value";
  receiptDigestHash?: string;
}

export interface SupperClubBookingInput {
  clubId: number;
  seats: number;
  pricePerSeatCents: number;
  rail?: "stripe_connect" | "native_usdc" | "evm_safe";
  preConditionEvidenceHash?: string;
}

export interface SupperClubBookingResult {
  bookingId: string;
  clubId: number;
  diner: string;
  seats: number;
  totalAmountCents: number;
  status: "Confirmed" | "Cancelled";
  splits: {
    hostTakeHomeCents: number;        // 93% to chef/host
    mutualShieldReserveCents: number; // 4% to mutual protection reserve
    platformFeeCents: number;         // 3% SaaS operator fee
  };
}

export interface BookingCancellationInput {
  bookingId: string;
  actualKitchenCostCents: number; // Compensate restaurant for perishable ingredients prepared
  punitiveLateFeeCents: number;   // Punitive deterrence hard-routed to charity sink
}

export interface TastemakerGuildInput {
  name: string;
  curatorAccount: string;
  curatorDomainBadge: string; // e.g. "sommelier_l3", "michelin_inspector", "nyc_food_critic"
  monthlyMembershipCents: number;
  legalWrapperHash: string;
}

