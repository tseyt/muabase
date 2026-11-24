export type TerminologyLocale = "institutional" | "muamalat";

export type MuabaseTransportMode = "subdomain" | "gateway";

export interface MuabaseConfig {
  endpoint: string; // psinode URL (e.g. http://127.0.0.1:8080 or https://node.muabase.org)
  locale?: TerminologyLocale;
  account?: string;
  /**
   * - `"subdomain"`: Native psinode virtual-hosted GraphQL (`http://<service>.<host>/graphql`).
   * - `"gateway"`: Reverse-proxy path routing (`<endpoint>/api/<service>/graphql`). Defaults to `"gateway"` for backward compatibility.
   */
  transport?: MuabaseTransportMode;
  keyPair?: {
    publicKey: string;
    privateKey?: string;
  };
}

export interface StateProofRequest {
  db?: "service" | "native";
  keyHex?: string;
  keyUtf8?: string;
  prefixHex?: string;
  prefixUtf8?: string;
}

export interface AnchoredMerkleProof {
  version: "muabase.dwal_merkle.v1";
  anchor: {
    blockNum: number;
    blockId: string;
    producer: string;
    stateRoot: string;
    headerSignature: string;
  };
  valueTag: number;
  valueHex: string;
  proof: {
    isInclusion: boolean;
    emptyTree: boolean;
    singleEntry: boolean;
    targetKeyHex: string;
    rootPrefixHex: string;
    witnessKeyHex: string;
    witnessCoreHash: string;
    steps: Array<{
      edgePrefixHex: string;
      hasTerminal: boolean;
      terminalHash: string;
      siblings: Array<{
        branchByte: number;
        isLeaf: boolean;
        edgePrefixHex: string;
        coreHash: string;
      }>;
    }>;
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

/**
 * =========================================================================
 * Humanitarian Remittance & Shariah-Compliant Zakat/Sadaqah Corridor Types
 * (NYDFS 23 NYCRR § 200.2(q) Non-Custodial / OFAC GL 6 Personal Remittance)
 * =========================================================================
 */

/**
 * Settlement routing mode for `ExternalVault` (`"ext-vault"`):
 * - `"COMMERCIAL_93_4_3"`: Standard commercial lease split (93% owner / 4% mutual_shield / 3% SaaS).
 * - `"ZAKAT_TAMLIK_100"`: 100% direct ownership transfer (*Tamlik*) of `zakatPrincipalAmount` to
 *   `recipientBridgeVault` with 0% fee deduction; optional `sadaqahTipAmount` routes separately.
 */
export type CorridorSplitMode = "COMMERCIAL_93_4_3" | "ZAKAT_TAMLIK_100";

/**
 * Zakat & humanitarian beneficiary eligibility classification (*Asnaf*, Qur'an 9:60).
 */
export type AsnafCategory =
  | "FUQARA"
  | "MASAKIN"
  | "GHARIMIN"
  | "IBN_AL_SABIL"
  | "GENERAL_SADAQAH";

/**
 * Supported non-custodial multi-signature & licensed bridge settlement rails (0 platform keys held).
 */
export type RemittanceSettlementRail =
  | "evm_safe"
  | "bitcoin_taproot"
  | "solana_squads"
  | "bridge_corridor_api";

/**
 * Input to propose a zero-PII `PersonalRemittanceAttestation` on `StagedTx` (`"staged-tx"`).
 */
export interface ProposeRemittanceAttestationInput {
  /** 32-byte SHA-256 salted commitment (`0x` + 64 hex chars) of off-chain encrypted beneficiary KYC/payout packet. */
  recipientCommitmentHash: string;
  /** Licensed bridge-country ISO-3166-1 alpha-2 jurisdiction code (e.g. `"TR"`, `"AE"`, `"UA"`, `"UZ"`). */
  corridorJurisdiction: string;
  /** 32-byte SHA-256 Ricardian text digest confirming non-commercial P2P personal remittance to a non-SDN individual. */
  nonCommercialPersonalDeclarationHash: string;
  /** Optional Zakat *Asnaf* category or `"GENERAL_SADAQAH"`. */
  asnafCategory?: AsnafCategory;
  /** Required independent witness quorum (enforced `>= 2`). Defaults to `2`. */
  witnessQuorum?: number;
}

/**
 * Input for a qualified community verifier (`trust_score >= 8000` + soulbound `"humanitarian-verifier"` badge)
 * to co-sign a zero-PII beneficiary commitment via `client.stagedTx.attestBeneficiaryCommitment(...)`.
 */
export interface AttestBeneficiaryCommitmentInput {
  /** Monotonic ID of the `PersonalRemittanceAttestation` record in `StagedTx`. */
  attestationId: number;
  /** 32-byte SHA-256 salted commitment (`0x` + 64 hex chars) matching the proposed beneficiary packet. */
  recipientCommitmentHash: string;
  /** Soulbound verifier domain credential. Defaults to `"humanitarian-verifier"`. */
  domainBadge?: string;
}

/**
 * On-chain zero-PII `PersonalRemittanceAttestation` state row in `StagedTx` (`RemittanceAttestationTable`).
 */
export interface PersonalRemittanceAttestation {
  attestationId: number;
  proposer: string;
  recipientCommitmentHash: string;
  corridorJurisdiction: string;
  nonCommercialPersonalDeclarationHash: string;
  asnafCategory?: AsnafCategory | null;
  witnessQuorum: number;
  requiredDomain: string;
  verifiedWitnesses: string[];
  quorumSatisfied: boolean;
  createdAt: string;
}

/**
 * Input to generate a non-custodial Zakat *Tamlik* or Humanitarian Corridor split intent on `ExternalVault` (`"ext-vault"`).
 */
export interface CreateZakatRemittanceSplitInput {
  /** Verified `PersonalRemittanceAttestation` ID with `witness_quorum >= 2` satisfied. */
  attestationId: number;
  /** Split invariant mode. Defaults to `"ZAKAT_TAMLIK_100"`. */
  mode?: CorridorSplitMode;
  /** Client-side multisig or licensed bridge settlement rail. */
  railType: RemittanceSettlementRail;
  /** Licensed bridge-country ISO-3166-1 alpha-2 code (e.g. `"TR"`, `"AE"`, `"UA"`, `"UZ"`). */
  corridorJurisdiction: string;
  /** Non-custodial recipient or licensed bridge Safe / Taproot / Squads settlement vault address. */
  recipientBridgeVault: string;
  /** Optional infrastructure / charity sink vault receiving the separate `sadaqahTipAmount`. */
  infraOpsVault?: string;
  /** Zakat principal in integer minor units (e.g. USDC cents or satoshis). Delivered 100% intact in `ZAKAT_TAMLIK_100`. */
  zakatPrincipalAmount: number;
  /** Optional separate donor *Sadaqah* tip funding gas, node ops, or bridge FX spread (`0` if omitted). */
  sadaqahTipAmount?: number;
  /** Protocol fee deduction from principal. MUST be `0` when `mode === "ZAKAT_TAMLIK_100"` or execution reverts. */
  protocolFeeDeduction?: number;
  /** 32-byte SHA-256 salted beneficiary commitment hash (`0x` + 64 hex chars). */
  recipientCommitmentHash: string;
}

/**
 * On-chain `ZakatRemittanceIntent` state row in `ExternalVault` (`RemittanceIntentTable`).
 */
export interface ZakatRemittanceIntent {
  intentId: number;
  attestationId: number;
  donor: string;
  mode: CorridorSplitMode;
  railType: RemittanceSettlementRail | string;
  corridorJurisdiction: string;
  recipientBridgeVault: string;
  infraOpsVault?: string | null;
  zakatPrincipalAmount: number;
  recipientAmount: number;
  sadaqahTipAmount: number;
  protocolFeeDeducted: number;
  recipientCommitmentHash: string;
  payloadJson: string;
  externalTxRef?: string | null;
  settled: boolean;
}

/**
 * Input to register a non-custodial licensed bridge-country exchange desk on `CreditClear` (`"cred-clear"`).
 */
export interface RegisterBridgeLiquidityDeskInput {
  corridorJurisdiction: string;
  deskLabel: string;
  /** 32-byte SHA-256 digest of exchange license / non-custodial attestation. */
  licenseRegistryHash: string;
  evmSafeAddress?: string;
  btcTaprootDescriptor?: string;
  solanaSquadsPda?: string;
  /** Live bridge FX spread in basis points (e.g. `85` = `0.85%`). */
  fxSpreadBps: number;
  maxPerTxUsdcCents: number;
  sadaqahFeeSubsidySupported?: boolean;
}

/**
 * Filter options for `client.creditClear.listBridgeCorridors(...)`.
 */
export interface ListBridgeCorridorsFilter {
  /** Optional 2-letter ISO bridge code (e.g. `"TR"`, `"AE"`, `"UA"`, `"UZ"`). */
  corridorJurisdiction?: string;
  /** When `true` (default), returns only active liquidity desks. */
  activeOnly?: boolean;
}

/**
 * Registered `BridgeLiquidityDesk` state row in `CreditClear` (`BridgeLiquidityDeskTable`).
 */
export interface BridgeLiquidityDesk {
  deskId: number;
  operator: string;
  corridorJurisdiction: string;
  deskLabel: string;
  licenseRegistryHash: string;
  evmSafeAddress: string;
  btcTaprootDescriptor: string;
  solanaSquadsPda: string;
  fxSpreadBps: number;
  maxPerTxUsdcCents: number;
  sadaqahFeeSubsidySupported: boolean;
  active: boolean;
  lastUpdated: string | number;
}

export interface CorridorConservation {
  corridorJurisdiction: string;
  settledCount: number;
  totalPrincipalDelivered: number;
  totalSadaqahTips: number;
  totalFeesDeducted: number;
}

export interface CreateEndowmentInput {
  label: string;
  corpusPrincipal: number;
  shurutHash: string;
  scholarAttestationHash: string;
  allowedCategories: string[];
  maxSingleDisbursement: number;
  boardMembers: string[];
  quorumThreshold: number;
}

export interface ProposeDisbursementInput {
  endowmentId: number;
  recipient: string;
  category: string;
  amount: number;
  evidenceCommitmentHash: string;
}

export interface AggregateTreasury {
  endowmentCount: number;
  totalCorpusLocked: number;
  totalYieldAvailable: number;
  totalDisbursed: number;
}

export interface RegisterTemplateInput {
  contractKind: "ISTISNA" | "MUDARABAH";
  templateHash: string;
  certificateHash: string;
  standardRef: string;
}

export interface CreateContractInput {
  templateId: number;
  expectedTemplateHash: string;
  contractor: string;
  arbitrators: [string, string, string];
  milestoneAmounts: number[];
  termsCommitmentHash: string;
}

export interface CastTahkimVoteInput {
  contractId: number;
  rulingKind: "AWARD_CONTRACTOR" | "REFUND_FUNDER" | "SPLIT";
  funderAmount?: number;
  contractorAmount?: number;
}


