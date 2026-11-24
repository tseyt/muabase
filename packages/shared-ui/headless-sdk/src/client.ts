import {
  MuabaseConfig,
  TerminologyLocale,
  AssetPoolConfig,
  CreateAssetInput,
  CheckoutLeaseInput,
  DamageClaimInput,
  UsufructLease,
  CoopPoolInput,
  RevenueReportInput,
  DistributionResult,
  CreateMarketInput,
  StakeAllocationInput,
  MarketPosition,
  MarketResolutionInput,
  ClaimRewardResult,
  RegisterRestaurantInput,
  ProofOfVisitInput,
  PairwiseComparisonInput,
  SupperClubBookingInput,
  SupperClubBookingResult,
  BookingCancellationInput,
  TastemakerGuildInput,
  ProposeRemittanceAttestationInput,
  AttestBeneficiaryCommitmentInput,
  PersonalRemittanceAttestation,
  CreateZakatRemittanceSplitInput,
  ZakatRemittanceIntent,
  RegisterBridgeLiquidityDeskInput,
  ListBridgeCorridorsFilter,
  BridgeLiquidityDesk,
  MuabaseTransportMode,
  StateProofRequest,
  AnchoredMerkleProof,
} from "./types";

const SHA256_HEX_RE = /^(?:0x)?([0-9a-fA-F]{64})$/;
const ZERO_SHA256_HEX = "0".repeat(64);
const ISO_CORRIDOR_RE = /^[A-Z]{2}$/;

/**
 * Client-side Zero-PII guard: ensures `digestHex` is an exact, non-zero 32-byte SHA-256 hex string
 * (`0x`-prefixed or 64 raw hex chars) and never plaintext beneficiary PII.
 */
export function assertValidSha256Commitment(digestHex: string, fieldName = "recipientCommitmentHash"): string {
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


export class MuabaseClient {
  public endpoint: string;
  public locale: TerminologyLocale;
  public account: string;
  public transport: MuabaseTransportMode;

  constructor(config: MuabaseConfig) {
    this.endpoint = config.endpoint.replace(/\/$/, "");
    this.locale = config.locale || "institutional";
    this.account = config.account || "anonymous";
    this.transport = config.transport || "gateway";
  }

  /**
   * Resolves a service GraphQL URL using either native psinode virtual-hosted subdomains
   * (`https://<service>.<host>/graphql`) or gateway path routing (`<endpoint>/api/<service>/graphql`).
   */
  public resolveGraphqlUrl(service: string): string {
    if (this.transport === "subdomain") {
      const parsed = new URL(this.endpoint);
      parsed.hostname = `${service}.${parsed.hostname}`;
      parsed.pathname = "/graphql";
      parsed.search = "";
      return parsed.toString();
    }
    return `${this.endpoint}/api/${service}/graphql`;
  }

  /**
   * Raw GraphQL query runner directly against psinode
   */
  public async query<T = any>(service: string, queryStr: string, variables?: Record<string, any>): Promise<T> {
    const url = this.resolveGraphqlUrl(service);
    const res = await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ query: queryStr, variables }),
    });

    if (!res.ok) {
      throw new Error(`Muabase Query Failed [${res.status}]: ${res.statusText}`);
    }

    const json = await res.json();
    if (json.errors && json.errors.length > 0) {
      throw new Error(`GraphQL Error: ${json.errors[0].message}`);
    }
    return json.data;
  }

  /**
   * Fetches an anchored `muabase.dwal_merkle.v1` JSON inclusion/non-inclusion proof
   * directly from psinode's native `POST /native/proof/state` endpoint.
   */
  public async fetchStateProof(request: StateProofRequest): Promise<AnchoredMerkleProof> {
    const res = await fetch(`${this.endpoint}/native/proof/state`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ db: "service", ...request }),
    });
    if (!res.ok) {
      throw new Error(`Muabase State Proof Failed [${res.status}]: ${await res.text()}`);
    }
    return (await res.json()) as AnchoredMerkleProof;
  }

  /**
   * Action submission via configured gateway proxy or local signer bridge
   */
  public async transact(service: string, method: string, args: Record<string, any>): Promise<any> {
    const url = `${this.endpoint}/api/${service}/action/${method}`;
    const res = await fetch(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(args),
    });

    if (!res.ok) {
      const err = await res.text();
      throw new Error(`Transaction Execution Reverted: ${err}`);
    }

    return await res.json();
  }

  /**
   * =========================================================================
   * Canonical Muabase Protocol Wasm Services
   * =========================================================================
   */

  public tokens = {
    /**
     * Configure an on-chain Asset Pool (Series LLC / Sukuk al-Qirad).
     * Wasm Service: "tokens", Action: "configureAssetPool"
     * @param config.tangibleRatioBps Basis points integer >= 5100 (51.00% tangible backing floor). 10000 = 100%. Never pass floats.
     * @param config.minTrustScore Minimum reliability score in basis points (0-10000). Default 7000.
     * @param config.priorityWindowSec Right of First Refusal window in seconds (e.g. 86400 = 24h).
     * @param config.charitySinkAccount Designated public charity sink vault (defaults to "waqf-sadaqah").
     */
    configureAssetPool: async (config: AssetPoolConfig) => {
      return this.transact("tokens", "configureAssetPool", { config });
    },
  };

  public leaseEngine = {
    /**
     * Create a usufruct lease for P2P rental or table booking (Ijarah al-Manafi').
     * Wasm Service: "lease-eng", Action: "createLease"
     * @param args.grossPayment.value Integer token units in precision 4 ($1.00 = 10000). Never pass raw floats.
     * @param args.preConditionEvidenceHash 0x-prefixed 32-byte SHA-256 hex digest string.
     * @param args.charitySinkAccount "waqf-sadaqah" for zero-riba late penalty routing.
     */
    createLease: async (args: Record<string, any>) => {
      return this.transact("lease-eng", "createLease", args);
    },
    /**
     * Confirm clean item return or dining conclusion, releasing deposits.
     * Wasm Service: "lease-eng", Action: "confirmCleanReturn"
     */
    confirmCleanReturn: async (args: { leaseId: number; postConditionEvidenceHash: string }) => {
      return this.transact("lease-eng", "confirmCleanReturn", args);
    },
    /**
     * Settle late return or cancellation: direct damage to host, punitive deterrence to charity sink.
     * Wasm Service: "lease-eng", Action: "settleCancellation"
     */
    settleCancellation: async (args: {
      leaseId: number;
      actualKitchenRestitution: { value: number };
      punitivePenalty: { value: number };
      charitySinkAccount: string;
    }) => {
      return this.transact("lease-eng", "settleCancellation", args);
    },
  };

  public stagedTx = {
    /**
     * Record a Ricardian evidence attestation with dual-witness Bayyinah gate.
     * Wasm Service: "staged-tx", Action: "recordAttestation"
     * @param args.evidenceHash 0x-prefixed 32-byte SHA-256 digest string of physical proof.
     * @param args.witnessQuorum Minimum number of co-signers required (default 2 for Bayyinah gate).
     */
    recordAttestation: async (args: {
      service: string;
      entityId: number;
      evidenceHash: string;
      attester?: string;
      witnessQuorum?: number;
      metadata?: any;
    }) => {
      return this.transact("staged-tx", "recordAttestation", args);
    },

    /**
     * Propose a zero-PII `PersonalRemittanceAttestation` Ricardian record (OFAC GL 6 / Non-Commercial P2P).
     * Wasm Service: `"staged-tx"`, Action: `"proposeRemittanceAttestation"`
     *
     * Stores only salted 32-byte SHA-256 digests (`recipientCommitmentHash` and
     * `nonCommercialPersonalDeclarationHash`) and enforces `witness_quorum >= 2` of verifiers
     * holding active `"humanitarian-verifier"` badges and `trust_score >= 8000`.
     */
    proposeRemittanceAttestation: async (
      input: ProposeRemittanceAttestationInput,
    ): Promise<{ attestationId: number }> => {
      const recipientCommitmentHash = assertValidSha256Commitment(
        input.recipientCommitmentHash,
        "recipientCommitmentHash",
      );
      const nonCommercialPersonalDeclarationHash = assertValidSha256Commitment(
        input.nonCommercialPersonalDeclarationHash,
        "nonCommercialPersonalDeclarationHash",
      );
      const corridorJurisdiction = assertValidCorridorJurisdiction(input.corridorJurisdiction);
      const witnessQuorum = input.witnessQuorum ?? 2;
      if (!Number.isInteger(witnessQuorum) || witnessQuorum < 2) {
        throw new Error(
          `Bayyinah Violation: PersonalRemittanceAttestation requires witnessQuorum >= 2, got ${witnessQuorum}`,
        );
      }

      return this.transact("staged-tx", "proposeRemittanceAttestation", {
        recipient_commitment_hash: recipientCommitmentHash,
        corridor_jurisdiction: corridorJurisdiction,
        non_commercial_personal_declaration_hash: nonCommercialPersonalDeclarationHash,
        asnaf_category: input.asnafCategory ?? null,
        witness_quorum: witnessQuorum,
      });
    },

    /**
     * Co-sign a zero-PII beneficiary commitment as an independent witness holding an active
     * soulbound `domain_badge` (`"humanitarian-verifier"`) and `trust_score >= 8000`.
     * Wasm Service: `"staged-tx"`, Action: `"attestBeneficiaryCommitment"`
     */
    attestBeneficiaryCommitment: async (
      input: AttestBeneficiaryCommitmentInput,
    ): Promise<{ attestationId: number; quorumSatisfied?: boolean }> => {
      const recipientCommitmentHash = assertValidSha256Commitment(
        input.recipientCommitmentHash,
        "recipientCommitmentHash",
      );
      const domainBadge = input.domainBadge ?? "humanitarian-verifier";

      return this.transact("staged-tx", "attestBeneficiaryCommitment", {
        attestation_id: input.attestationId,
        recipient_commitment_hash: recipientCommitmentHash,
        domain_badge: domainBadge,
      });
    },

    /**
     * Query a single `PersonalRemittanceAttestation` quorum state record via GraphQL.
     * Wasm Query Service: `"staged-tx"` (`RemittanceAttestationTable`)
     */
    getRemittanceAttestation: async (
      attestationId: number,
    ): Promise<PersonalRemittanceAttestation | null> => {
      const data = await this.query<{
        remittanceAttestation: PersonalRemittanceAttestation | null;
      }>(
        "staged-tx",
        `query GetRemittanceAttestation($attestationId: Int!) {
          remittanceAttestation(attestationId: $attestationId) {
            attestationId
            proposer
            recipientCommitmentHash
            corridorJurisdiction
            nonCommercialPersonalDeclarationHash
            asnafCategory
            witnessQuorum
            requiredDomain
            verifiedWitnesses
            quorumSatisfied
            createdAt
          }
        }`,
        { attestationId },
      );
      return data.remittanceAttestation ?? null;
    },
  };

  public evaluations = {
    /**
     * Record a pairwise Condorcet tournament ballot (Ju'alah Halaqa).
     * Wasm Service: "evaluation"
     */
    recordPairwiseVote: async (args: {
      winnerId: number;
      loserId: number;
      category?: string;
      voter?: string;
      receiptHash?: string | null;
    }) => {
      return this.transact("evaluation", "recordPairwiseVote", args);
    },
    "createJu'alahBounty": async (args: Record<string, any>) => {
      return this.transact("evaluation", "createJu'alahBounty", args);
    },
    createJualahBounty: async (args: Record<string, any>) => {
      return this.transact("evaluation", "createJu'alahBounty", args);
    },
    "resolveJu'alahBounty": async (args: Record<string, any>) => {
      return this.transact("evaluation", "resolveJu'alahBounty", args);
    },
    resolveJualahBounty: async (args: Record<string, any>) => {
      return this.transact("evaluation", "resolveJu'alahBounty", args);
    },
  };

  public claimCouncil = {
    /**
     * Submit a physical damage claim to 3-peer rapid triage committee.
     * Wasm Service: "claim-coun", Action: "submitDamageClaim"
     */
    submitDamageClaim: async (args: Record<string, any>) => {
      return this.transact("claim-coun", "submitDamageClaim", args);
    },
    voteDamageClaim: async (args: { claimId: number; approve: boolean; approvedPayout: { value: number } }) => {
      return this.transact("claim-coun", "voteDamageClaim", args);
    },
  };

  public externalVault = {
    /**
     * Generate non-custodial multi-party split intent (93% Owner / 4% Takaful / 3% SaaS).
     * Wasm Service: "ext-vault", Action: "generateSplitIntent"
     * @param args.grossAmountCents Integer currency cents ($1.00 = 100).
     */
    generateSplitIntent: async (args: Record<string, any>) => {
      return this.transact("ext-vault", "generateSplitIntent", args);
    },
    generateInvestmentIntent: async (args: Record<string, any>) => {
      return this.transact("ext-vault", "generateInvestmentIntent", args);
    },

    /**
     * Generate a non-custodial Zakat *Tamlik* (`ZAKAT_TAMLIK_100`) or Humanitarian Corridor split intent.
     * Wasm Service: `"ext-vault"`, Action: `"createZakatRemittanceSplit"`
     */
    createZakatRemittanceSplit: async (
      input: CreateZakatRemittanceSplitInput,
    ): Promise<ZakatRemittanceIntent & { intentId: number }> => {
      const recipientCommitmentHash = assertValidSha256Commitment(
        input.recipientCommitmentHash,
        "recipientCommitmentHash",
      );
      const corridorJurisdiction = assertValidCorridorJurisdiction(input.corridorJurisdiction);
      if (!input.recipientBridgeVault || input.recipientBridgeVault.trim().length === 0) {
        throw new Error(
          "recipientBridgeVault must be a non-empty non-custodial Safe, Taproot, Squads, or licensed bridge address",
        );
      }
      const { mode, zakatPrincipalAmount, sadaqahTipAmount, protocolFeeDeduction } =
        assertZakatTamlikSplitInvariant(input);

      return this.transact("ext-vault", "createZakatRemittanceSplit", {
        attestation_id: input.attestationId,
        mode,
        rail_type: input.railType,
        corridor_jurisdiction: corridorJurisdiction,
        recipient_bridge_vault: input.recipientBridgeVault,
        infra_ops_vault: input.infraOpsVault ?? null,
        zakat_principal_amount: zakatPrincipalAmount,
        sadaqah_tip_amount: sadaqahTipAmount,
        protocol_fee_deduction: protocolFeeDeduction,
        recipient_commitment_hash: recipientCommitmentHash,
      });
    },

    /**
     * Confirm external non-custodial settlement of a Zakat / Humanitarian remittance split.
     * Wasm Service: `"ext-vault"`, Action: `"confirmRemittanceSettlement"`
     */
    confirmRemittanceSettlement: async (args: {
      intentId: number;
      externalTxRef: string;
    }): Promise<{ intentId: number; settled: boolean; externalTxRef: string }> => {
      if (!args.externalTxRef || args.externalTxRef.trim().length === 0) {
        throw new Error("externalTxRef settlement confirmation hash must be non-empty");
      }
      return this.transact("ext-vault", "confirmRemittanceSettlement", {
        intent_id: args.intentId,
        external_tx_ref: args.externalTxRef,
      });
    },

    /**
     * Query aggregate conservation totals for a humanitarian corridor (zero PII).
     * Wasm Service: `"ext-vault"`, GraphQL Query: `corridorConservation`
     */
    getCorridorConservation: async (corridorJurisdiction: string) => {
      assertValidCorridorJurisdiction(corridorJurisdiction);
      const data = await this.query(
        "ext-vault",
        `query CorridorConservation($corridorJurisdiction: String!) {
          corridorConservation(corridorJurisdiction: $corridorJurisdiction) {
            corridorJurisdiction
            settledCount
            totalPrincipalDelivered
            totalSadaqahTips
            totalFeesDeducted
          }
        }`,
        { corridorJurisdiction },
      );
      return data.corridorConservation;
    },
  };

  public creditClear = {
    /**
     * Execute bilateral B2B invoice set-off (Muqassah).
     * Wasm Service: "cred-clear", Action: "executeBilateralNetting"
     */
    executeBilateralNetting: async (args: { invoice1Id: number; invoice2Id: number }) => {
      return this.transact("cred-clear", "executeBilateralNetting", args);
    },

    /**
     * Register a licensed bridge-country liquidity desk publishing non-custodial vaults.
     * Wasm Service: `"cred-clear"`, Action: `"registerBridgeLiquidityDesk"`
     */
    registerBridgeLiquidityDesk: async (
      input: RegisterBridgeLiquidityDeskInput,
    ): Promise<{ deskId: number }> => {
      const corridorJurisdiction = assertValidCorridorJurisdiction(input.corridorJurisdiction);
      const licenseRegistryHash = assertValidSha256Commitment(
        input.licenseRegistryHash,
        "licenseRegistryHash",
      );
      const evmSafeAddress = input.evmSafeAddress ?? "";
      const btcTaprootDescriptor = input.btcTaprootDescriptor ?? "";
      const solanaSquadsPda = input.solanaSquadsPda ?? "";
      if (
        evmSafeAddress.trim().length === 0 &&
        btcTaprootDescriptor.trim().length === 0 &&
        solanaSquadsPda.trim().length === 0
      ) {
        throw new Error(
          "Non-Custodial Safe Harbor Violation: BridgeLiquidityDesk must publish at least one Safe, Taproot, or Squads multisig address",
        );
      }

      return this.transact("cred-clear", "registerBridgeLiquidityDesk", {
        corridor_jurisdiction: corridorJurisdiction,
        desk_label: input.deskLabel,
        license_registry_hash: licenseRegistryHash,
        evm_safe_address: evmSafeAddress,
        btc_taproot_descriptor: btcTaprootDescriptor,
        solana_squads_pda: solanaSquadsPda,
        fx_spread_bps: input.fxSpreadBps,
        max_per_tx_usdc_cents: input.maxPerTxUsdcCents,
        sadaqah_fee_subsidy_supported: input.sadaqahFeeSubsidySupported ?? true,
      });
    },

    /**
     * Discover active licensed non-custodial bridge-country liquidity desks and live FX spreads.
     * Wasm Service: `"cred-clear"`, GraphQL Query: `listBridgeCorridors`
     */
    listBridgeCorridors: async (
      filter: ListBridgeCorridorsFilter = {},
    ): Promise<BridgeLiquidityDesk[]> => {
      if (filter.corridorJurisdiction !== undefined) {
        assertValidCorridorJurisdiction(filter.corridorJurisdiction);
      }
      const data = await this.query<{ listBridgeCorridors: BridgeLiquidityDesk[] }>(
        "cred-clear",
        `query ListBridgeCorridors($corridorJurisdiction: String, $activeOnly: Boolean) {
          listBridgeCorridors(corridorJurisdiction: $corridorJurisdiction, activeOnly: $activeOnly) {
            deskId
            operator
            corridorJurisdiction
            deskLabel
            licenseRegistryHash
            evmSafeAddress
            btcTaprootDescriptor
            solanaSquadsPda
            fxSpreadBps
            maxPerTxUsdcCents
            sadaqahFeeSubsidySupported
            active
            lastUpdated
          }
        }`,
        {
          corridorJurisdiction: filter.corridorJurisdiction ?? null,
          activeOnly: filter.activeOnly ?? true,
        },
      );
      return data.listBridgeCorridors ?? [];
    },
  };

  public waqfTreasury = {
    /**
     * Create a rule-locked Waqf / Masjid endowment (`waqf-treas::createEndowment`).
     */
    createEndowment: async (args: {
      label: string;
      corpusPrincipal: number;
      shurutHash: string;
      scholarAttestationHash: string;
      allowedCategories: string[];
      maxSingleDisbursement: number;
      boardMembers: string[];
      quorumThreshold: number;
    }) => {
      return this.transact("waqf-treas", "createEndowment", {
        label: args.label,
        corpus_principal: args.corpusPrincipal,
        shurut_hash: assertValidSha256Commitment(args.shurutHash, "shurutHash"),
        scholar_attestation_hash: assertValidSha256Commitment(
          args.scholarAttestationHash,
          "scholarAttestationHash",
        ),
        allowed_categories: args.allowedCategories,
        max_single_disbursement: args.maxSingleDisbursement,
        board_members: args.boardMembers,
        quorum_threshold: args.quorumThreshold,
      });
    },
    proposeDisbursement: async (args: {
      endowmentId: number;
      recipient: string;
      category: string;
      amount: number;
      evidenceCommitmentHash: string;
    }) => {
      return this.transact("waqf-treas", "proposeDisbursement", {
        endowment_id: args.endowmentId,
        recipient: args.recipient,
        category: args.category,
        amount: args.amount,
        evidence_commitment_hash: assertValidSha256Commitment(
          args.evidenceCommitmentHash,
          "evidenceCommitmentHash",
        ),
      });
    },
    approveDisbursement: async (proposalId: number) => {
      return this.transact("waqf-treas", "approveDisbursement", { proposal_id: proposalId });
    },
    getAggregateTreasury: async () => {
      const data = await this.query(
        "waqf-treas",
        `{ aggregateTreasury { endowmentCount totalCorpusLocked totalYieldAvailable totalDisbursed } }`,
      );
      return data.aggregateTreasury;
    },
  };

  public muamalatTahkim = {
    /**
     * Register a scholar-attested commercial contract template (`muamalat::registerTemplate`).
     */
    registerTemplate: async (args: {
      contractKind: "ISTISNA" | "MUDARABAH";
      templateHash: string;
      certificateHash: string;
      standardRef: string;
    }) => {
      return this.transact("muamalat", "registerTemplate", {
        contract_kind: args.contractKind,
        template_hash: assertValidSha256Commitment(args.templateHash, "templateHash"),
        certificate_hash: assertValidSha256Commitment(args.certificateHash, "certificateHash"),
        standard_ref: args.standardRef,
      });
    },
    createContract: async (args: {
      templateId: number;
      expectedTemplateHash: string;
      contractor: string;
      arbitrators: [string, string, string];
      milestoneAmounts: number[];
      termsCommitmentHash: string;
    }) => {
      return this.transact("muamalat", "createContract", {
        template_id: args.templateId,
        expected_template_hash: assertValidSha256Commitment(
          args.expectedTemplateHash,
          "expectedTemplateHash",
        ),
        contractor: args.contractor,
        arbitrators: args.arbitrators,
        milestone_amounts: args.milestoneAmounts,
        terms_commitment_hash: assertValidSha256Commitment(
          args.termsCommitmentHash,
          "termsCommitmentHash",
        ),
      });
    },
    castTahkimVote: async (args: {
      contractId: number;
      rulingKind: "AWARD_CONTRACTOR" | "REFUND_FUNDER" | "SPLIT";
      funderAmount?: number;
      contractorAmount?: number;
    }) => {
      return this.transact("muamalat", "castTahkimVote", {
        contract_id: args.contractId,
        ruling_kind: args.rulingKind,
        funder_amount: args.funderAmount ?? 0,
        contractor_amount: args.contractorAmount ?? 0,
      });
    },
  };

  public tokenStream = {
    recordGrossRevenue: async (args: Record<string, any>) => {
      return this.transact("tok-stream", "recordGrossRevenue", args);
    },
    distributeCoopYield: async (args: Record<string, any>) => {
      return this.transact("tok-stream", "distributeCoopYield", args);
    },
    allocateYieldStake: async (args: Record<string, any>) => {
      return this.transact("tok-stream", "allocateYieldStake", args);
    },
    "claimJu'alahReward": async (args: Record<string, any>) => {
      return this.transact("tok-stream", "claimJu'alahReward", args);
    },
    claimJualahReward: async (args: Record<string, any>) => {
      return this.transact("tok-stream", "claimJu'alahReward", args);
    },
  };

  /**
   * =========================================================================
   * Convenience Domain Namespaces (Delegating to Canonical Services)
   * =========================================================================
   */

  /**
   * Guild domain namespace
   */
  public guild = {
    assets: {
      create: async (input: CreateAssetInput) => {
        return this.transact("tokens", "configureAssetPool", {
          config: {
            tokenId: 1042,
            legalWrapperHash: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            maxHolders: input.maxHolders || 99,
            minTrustScore: input.minTrustScore ?? input.minAdalahScore ?? 7000,
            tangibleRatioBps: input.tangibleRatioBps || 10000,
            priorityWindowSec: input.priorityWindowSec ?? input.rofrWindowSec ?? 86400,
            custodianAccount: input.custodianAccount || "custodian",
            custodianFeeBps: input.custodianFeeBps || 0,
            charitySinkAccount: "waqf-sadaqah",
          },
        });
      },
    },

    leases: {
      checkout: async (input: CheckoutLeaseInput): Promise<UsufructLease> => {
        const grossPayment = (input.durationDays * 45) * 10000; // $45/day in precision 4
        const result = await this.transact("lease-eng", "createLease", {
          lessor: "asset-owner",
          tokenId: input.assetId,
          nftId: null,
          durationSec: input.durationDays * 86400,
          grossPayment: { value: grossPayment },
          paymentTokenId: 1, // USD Stablecoin / USDC
          preConditionEvidenceHash: input.preConditionEvidenceHash,
          isProgressiveBuyout: input.isProgressiveBuyout || false,
          buyoutEquityUnits: { value: input.buyoutEquityUnits || 0 },
          custodianAccount: "custodian",
          custodianFeeBps: 0,
          charitySinkAccount: "waqf-sadaqah",
        });

        // If paying via mainstream Stripe Connect fiat rail, trigger non-custodial destination split intent
        if (input.rail === "stripe_connect") {
          await this.transact("ext-vault", "generateSplitIntent", {
            leaseId: result.leaseId || 101,
            railType: "stripe_connect",
            grossAmountCents: (input.durationDays * 45) * 100, // cents
            lenderAccount: "asset-owner",
          });
        }

        return {
          leaseId: String(result.leaseId || 101),
          lessor: "asset-owner",
          lessee: this.account,
          tokenId: input.assetId,
          durationSec: input.durationDays * 86400,
          grossPayment,
          status: "Active",
          preConditionEvidenceHash: input.preConditionEvidenceHash,
          isProgressiveBuyout: input.isProgressiveBuyout || false,
          buyoutEquityUnits: input.buyoutEquityUnits || 0,
        };
      },

      confirmReturn: async (input: { leaseId: string; postConditionEvidenceHash: string }) => {
        return this.transact("lease-eng", "confirmCleanReturn", {
          leaseId: parseInt(input.leaseId, 10),
          postConditionEvidenceHash: input.postConditionEvidenceHash,
        });
      },
    },

    claims: {
      submitEvidence: async (input: DamageClaimInput) => {
        return this.transact("claimcouncil", "submitDamageClaim", {
          leaseId: parseInt(input.leaseId, 10),
          respondent: "renter-account",
          poolId: 1,
          claimedAmount: { value: input.claimedAmountUsd * 10000 },
          preConditionEvidenceHash: input.preConditionEvidenceHash,
          postConditionEvidenceHash: input.postConditionEvidenceHash,
          repairQuoteHash: input.repairQuoteHash,
        });
      },

      voteTriage: async (input: { claimId: string; approve: boolean; payoutUsd: number }) => {
        return this.transact("claimcouncil", "voteDamageClaim", {
          claimId: parseInt(input.claimId, 10),
          approve: input.approve,
          approvedPayout: { value: input.payoutUsd * 10000 },
        });
      },
    },

    credit: {
      netInvoices: async (invoice1: string, invoice2: string) => {
        return this.transact("creditclear", "executeBilateralNetting", {
          invoice1Id: parseInt(invoice1, 10),
          invoice2Id: parseInt(invoice2, 10),
        });
      },
    },
  };

  /**
   * Real Estate & Infrastructure Digital Cooperative namespace (MizanVest)
   */
  public coop = {
    pools: {
      create: async (input: CoopPoolInput) => {
        return this.transact("tokens", "configureAssetPool", {
          config: {
            tokenId: 2001,
            legalWrapperHash: input.legalWrapperHash,
            maxHolders: 499,
            minTrustScore: 6000,
            tangibleRatioBps: input.tangibleRatioBps || 10000,
            priorityWindowSec: input.priorityWindowSec || 86400,
            custodianAccount: input.custodianAccount || "mizan-custodian",
            custodianFeeBps: 200, // 2% annual management fee
            charitySinkAccount: "waqf-sadaqah",
          },
        });
      },

      reportRevenue: async (input: RevenueReportInput) => {
        return this.transact("tokenstream", "recordGrossRevenue", {
          poolId: input.poolId,
          source: input.source,
          grossAmountCents: input.grossAmountCents,
          operatingExpensesCents: input.operatingExpensesCents,
          evidenceHash: input.evidenceHash,
        });
      },

      executeStabilizedDistribution: async (params: {
        poolId: number;
        grossRevenueCents: number;
        operatingExpensesCents: number;
        portfolioValuationCents: number;
      }): Promise<DistributionResult> => {
        const netIncomeCents = params.grossRevenueCents - params.operatingExpensesCents;
        const grossYieldBps = Math.floor((params.grossRevenueCents * 10000) / params.portfolioValuationCents);
        const netYieldBps = Math.floor((netIncomeCents * 10000) / params.portfolioValuationCents);

        let effectivePayoutBps = netYieldBps;
        let reserveSurplusAddedCents = 0;
        let reserveDrawCents = 0;

        if (netYieldBps > 1200) {
          // Outperformance: Cap distribution at 12%, excess routes to Yield Stabilization Fund
          effectivePayoutBps = 1200;
          reserveSurplusAddedCents = Math.floor(((netYieldBps - 1200) * params.portfolioValuationCents) / 10000);
        } else if (netYieldBps < 800) {
          // Underperformance: Draw from Yield Stabilization Fund up to 8% target floor
          reserveDrawCents = Math.floor(((800 - netYieldBps) * params.portfolioValuationCents) / 10000);
          effectivePayoutBps = 800;
        }

        const res = await this.transact("tokenstream", "distributeCoopYield", {
          poolId: params.poolId,
          netYieldBps,
          effectivePayoutBps,
          reserveSurplusAddedCents,
          reserveDrawCents,
        });

        return {
          distributionId: String(res.distributionId || "dist-1"),
          poolId: params.poolId,
          grossYieldBps,
          netYieldBps,
          effectivePayoutBps,
          reserveSurplusAddedCents,
          reserveDrawCents,
          reserveTotalBalanceCents: res.reserveTotalBalanceCents || 25000000,
        };
      },
    },

    shares: {
      invest: async (input: { poolId: number; amountCents: number; rail?: string }) => {
        if (input.rail === "stripe_connect") {
          return await this.transact("ext-vault", "generateInvestmentIntent", {
            poolId: input.poolId,
            amountCents: input.amountCents,
            railType: "stripe_connect",
            investorAccount: this.account,
          });
        }

        return await this.transact("tokens", "purchaseDigitalShares", {
          poolId: input.poolId,
          amountCents: input.amountCents,
          buyer: this.account,
        });
      },
    },
  };

  /**
   * Ethical Forecasting & Ju'alah Knowledge Bounty namespace (Irfan.Markets)
   */
  public forecasting = {
    markets: {
      create: async (input: CreateMarketInput) => {
        return this.transact("evals", "createJu'alahBounty", {
          question: input.question,
          category: input.category,
          outcomes: input.outcomes,
          resolutionTimestamp: input.resolutionTimestamp,
          oracleAccount: input.oracleAccount,
          minTrustScore: input.minTrustScore || 5000,
          descriptionHash: input.descriptionHash,
          charitySinkAccount: "waqf-sadaqah",
        });
      },

      resolve: async (input: MarketResolutionInput) => {
        return this.transact("evals", "resolveJu'alahBounty", {
          marketId: input.marketId,
          winningOutcomeIndex: input.winningOutcomeIndex,
          evidenceHash: input.evidenceHash,
          dualWitnessSignatures: input.dualWitnessSignatures || [],
        });
      },
    },

    vault: {
      allocateStake: async (input: StakeAllocationInput): Promise<MarketPosition> => {
        const result = await this.transact("tokenstream", "allocateYieldStake", {
          marketId: input.marketId,
          outcomeIndex: input.outcomeIndex,
          principalAmountUsd: input.principalAmountUsd,
          staker: this.account,
          rail: input.rail || "native_usdc",
        });

        return {
          positionId: String(result.positionId || "pos-1"),
          marketId: input.marketId,
          account: this.account,
          outcomeIndex: input.outcomeIndex,
          principalAmountUsd: input.principalAmountUsd,
          prizeShares: input.principalAmountUsd * 86400, // Amount * Duration (stake_seconds)
          allocationTimestamp: Math.floor(Date.now() / 1000),
          status: "Active",
        };
      },

      claim: async (positionId: string): Promise<ClaimRewardResult> => {
        const res = await this.transact("tokenstream", "claimJu'alahReward", {
          positionId: parseInt(positionId, 10),
          claimer: this.account,
        });

        return {
          positionId,
          principalRefundUsd: res.principalRefundUsd, // 100% of user principal returned
          yieldPrizeRewardUsd: res.yieldPrizeRewardUsd || 0, // Pro-rata share of redirected yield
          skillPointsEarned: res.skillPointsEarned || 250, // Soulbound trust_score addition
        };
      },
    },
  };

  /**
   * Social Restaurant Tracking & Tastemaker Guild namespace (Beli Challenger)
   */
  public dining = {
    restaurants: {
      register: async (input: RegisterRestaurantInput) => {
        return this.transact("tokens", "configureAssetPool", {
          config: {
            tokenId: 3001,
            legalWrapperHash: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            maxHolders: 10000,
            minTrustScore: input.minTrustScore || 6500,
            tangibleRatioBps: input.tangibleRatioBps || 10000,
            priorityWindowSec: 86400,
            custodianAccount: "restaurant-registry",
            custodianFeeBps: 0,
            charitySinkAccount: "waqf-sadaqah",
          },
        });
      },

      submitProofOfVisit: async (input: ProofOfVisitInput) => {
        return this.transact("staged-tx", "recordAttestation", {
          service: "evaluation",
          entityId: input.restaurantId,
          evidenceHash: input.receiptDigestHash,
          attester: input.dinerAccount || this.account,
          witnessQuorum: input.witnessAccounts?.length || 1,
          metadata: {
            overallRatingBps: input.overallRatingBps,
            dishes: input.dishesOrdered,
            timestamp: input.visitTimestamp,
          },
        });
      },
    },

    rankings: {
      recordComparison: async (input: PairwiseComparisonInput) => {
        return this.transact("evals", "recordPairwiseVote", {
          winnerId: input.winnerRestaurantId,
          loserId: input.loserRestaurantId,
          category: input.category || "overall",
          voter: this.account,
          receiptHash: input.receiptDigestHash || null,
        });
      },
    },

    supperClubs: {
      book: async (input: SupperClubBookingInput): Promise<SupperClubBookingResult> => {
        const totalAmountCents = input.seats * input.pricePerSeatCents;
        const result = await this.transact("lease-eng", "createLease", {
          lessor: "host-chef",
          tokenId: input.clubId,
          nftId: null,
          durationSec: 14400, // 4-hour dining usufruct
          grossPayment: { value: totalAmountCents * 100 },
          paymentTokenId: 1,
          preConditionEvidenceHash: input.preConditionEvidenceHash || "0x0",
          isProgressiveBuyout: false,
          buyoutEquityUnits: { value: 0 },
          custodianAccount: "supper-host",
          custodianFeeBps: 0,
          charitySinkAccount: "waqf-sadaqah",
        });

        if (input.rail === "stripe_connect") {
          await this.transact("ext-vault", "generateSplitIntent", {
            leaseId: result.leaseId || 401,
            railType: "stripe_connect",
            grossAmountCents: totalAmountCents,
            lenderAccount: "host-chef",
          });
        }

        return {
          bookingId: String(result.leaseId || 401),
          clubId: input.clubId,
          diner: this.account,
          seats: input.seats,
          totalAmountCents,
          status: "Confirmed",
          splits: {
            hostTakeHomeCents: Math.floor(totalAmountCents * 0.93),
            mutualShieldReserveCents: Math.floor(totalAmountCents * 0.04),
            platformFeeCents: Math.floor(totalAmountCents * 0.03),
          },
        };
      },

      cancelWithEthicalDeterrence: async (input: BookingCancellationInput) => {
        return this.transact("lease-eng", "settleCancellation", {
          leaseId: parseInt(input.bookingId, 10),
          actualKitchenRestitution: { value: input.actualKitchenCostCents * 100 },
          punitivePenalty: { value: input.punitiveLateFeeCents * 100 },
          charitySinkAccount: "waqf-sadaqah",
        });
      },
    },

    guilds: {
      createTastemakerGuild: async (input: TastemakerGuildInput) => {
        return this.transact("guilds", "createPod", {
          name: input.name,
          curator: input.curatorAccount,
          requiredDomainBadge: input.curatorDomainBadge,
          membershipFeeCents: input.monthlyMembershipCents,
          splits: {
            curatorBps: 9300,
            mutualShieldBps: 400,
            platformBps: 300,
          },
        });
      },
    },
  };
}

export function createMuabaseClient(config: MuabaseConfig): MuabaseClient {
  return new MuabaseClient(config);
}
