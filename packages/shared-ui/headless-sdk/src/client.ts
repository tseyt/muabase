import type {
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
  CorridorConservation,
  CreateEndowmentInput,
  ProposeDisbursementInput,
  AggregateTreasury,
  RegisterTemplateInput,
  CreateContractInput,
  CastTahkimVoteInput,
  MuabaseTransportMode,
  StateProofRequest,
  AnchoredMerkleProof,
} from "./types.js";
import { MUABASE_SERVICES } from "./services.js";
import {
  assertPositiveInteger,
  assertValidCorridorJurisdiction,
  assertValidSha256Commitment,
  assertZakatTamlikSplitInvariant,
  computeCommercialSplitCents,
} from "./guards.js";

export {
  MUABASE_SERVICES,
  assertPositiveInteger,
  assertValidCorridorJurisdiction,
  assertValidSha256Commitment,
  assertZakatTamlikSplitInvariant,
  computeCommercialSplitCents,
};

function assertValidEndpointUrl(endpoint: string): string {
  let parsed: URL;
  try {
    parsed = new URL(endpoint);
  } catch {
    throw new Error(`Invalid Muabase endpoint URL: '${endpoint}'`);
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new Error(`Muabase endpoint URL must use http:// or https://, got '${parsed.protocol}'`);
  }
  return endpoint.replace(/\/$/, "");
}

export class MuabaseClient {
  public endpoint: string;
  public locale: TerminologyLocale;
  public account: string;
  public transport: MuabaseTransportMode;

  constructor(config: MuabaseConfig) {
    this.endpoint = assertValidEndpointUrl(config.endpoint);
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
      const isRawIp =
        /^(\d{1,3}\.){3}\d{1,3}$/.test(parsed.hostname) ||
        parsed.hostname.startsWith("[") ||
        parsed.hostname.includes(":");
      if (isRawIp) {
        throw new Error(
          `Subdomain transport cannot prepend "${service}." to IP endpoint "${parsed.hostname}". ` +
            `Use a hostname like "http://muabase.localhost:${parsed.port || 8080}" or transport: "gateway".`,
        );
      }
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
      const body = await res.text().catch(() => "");
      throw new Error(
        `Muabase Query Failed [${res.status} ${res.statusText}] (${url}): ${body || "empty response"}`,
      );
    }

    const json = await res.json();
    if (json.errors && json.errors.length > 0) {
      const messages = json.errors.map((e: any) => e.message).join("; ");
      throw new Error(`GraphQL Error: ${messages}`);
    }
    return json.data;
  }

  /**
   * Fetches an anchored `muabase.dwal_merkle.v1` JSON inclusion/non-inclusion proof
   * directly from psinode's native `POST /native/proof/state` endpoint.
   */
  public async fetchStateProof(request: StateProofRequest): Promise<AnchoredMerkleProof> {
    if (!request.keyHex && request.keyUtf8 === undefined) {
      throw new Error("StateProofRequest requires either keyHex or keyUtf8");
    }
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
      const err = await res.text().catch(() => "");
      throw new Error(
        `Transaction Execution Reverted [${res.status}] (${service}::${method}): ${err || res.statusText}`,
      );
    }

    return await res.json();
  }

  /**
   * Canonical Branding & Dual-Locale Terminology namespace ("branding")
   */
  public branding = {
    getNetworkName: async (): Promise<string> => {
      const data = await this.query<{ networkName: string }>(
        "branding",
        "{ networkName }",
      );
      return data.networkName;
    },

    getTerminologyLocale: async (): Promise<TerminologyLocale> => {
      const data = await this.query<{ terminologyLocale: TerminologyLocale }>(
        "branding",
        "{ terminologyLocale }",
      );
      return data.terminologyLocale;
    },

    getTerminologyDictionary: async (): Promise<
      Array<{ key: string; institutional: string; muamalat: string }>
    > => {
      const data = await this.query<{
        terminologyDictionary: Array<{
          key: string;
          institutional: string;
          muamalat: string;
        }>;
      }>("branding", "{ terminologyDictionary { key institutional muamalat } }");
      return data.terminologyDictionary;
    },

    translateTerm: async (term: string): Promise<string> => {
      const data = await this.query<{ translateTerm: string }>(
        "branding",
        "query ($term: String!) { translateTerm(term: $term) }",
        { term },
      );
      return data.translateTerm;
    },
  };

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
      return this.transact(MUABASE_SERVICES.TOKENS, "configureAssetPool", { config });
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
      return this.transact(MUABASE_SERVICES.LEASE_ENGINE, "createLease", args);
    },
    /**
     * Confirm clean item return or dining conclusion, releasing deposits.
     * Wasm Service: "lease-eng", Action: "confirmCleanReturn"
     */
    confirmCleanReturn: async (args: { leaseId: number; postConditionEvidenceHash: string }) => {
      return this.transact(MUABASE_SERVICES.LEASE_ENGINE, "confirmCleanReturn", args);
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
      return this.transact(MUABASE_SERVICES.LEASE_ENGINE, "settleCancellation", args);
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
      return this.transact(MUABASE_SERVICES.STAGED_TX, "recordAttestation", args);
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

      return this.transact(MUABASE_SERVICES.STAGED_TX, "proposeRemittanceAttestation", {
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

      return this.transact(MUABASE_SERVICES.STAGED_TX, "attestBeneficiaryCommitment", {
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
        MUABASE_SERVICES.STAGED_TX,
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
      return this.transact(MUABASE_SERVICES.EVALUATIONS, "recordPairwiseVote", args);
    },
    "createJu'alahBounty": async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.EVALUATIONS, "createJu'alahBounty", args);
    },
    createJualahBounty: async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.EVALUATIONS, "createJu'alahBounty", args);
    },
    "resolveJu'alahBounty": async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.EVALUATIONS, "resolveJu'alahBounty", args);
    },
    resolveJualahBounty: async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.EVALUATIONS, "resolveJu'alahBounty", args);
    },
  };

  public claimCouncil = {
    /**
     * Submit a physical damage claim to 3-peer rapid triage committee.
     * Wasm Service: "claim-coun", Action: "submitDamageClaim"
     */
    submitDamageClaim: async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.CLAIM_COUNCIL, "submitDamageClaim", args);
    },
    voteDamageClaim: async (args: { claimId: number; approve: boolean; approvedPayout: { value: number } }) => {
      return this.transact(MUABASE_SERVICES.CLAIM_COUNCIL, "voteDamageClaim", args);
    },
  };

  public externalVault = {
    /**
     * Generate non-custodial multi-party split intent (93% Owner / 4% Takaful / 3% SaaS).
     * Wasm Service: "ext-vault", Action: "generateSplitIntent"
     * @param args.grossAmountCents Integer currency cents ($1.00 = 100).
     */
    generateSplitIntent: async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.EXTERNAL_VAULT, "generateSplitIntent", args);
    },
    generateInvestmentIntent: async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.EXTERNAL_VAULT, "generateInvestmentIntent", args);
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

      return this.transact(MUABASE_SERVICES.EXTERNAL_VAULT, "createZakatRemittanceSplit", {
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
      return this.transact(MUABASE_SERVICES.EXTERNAL_VAULT, "confirmRemittanceSettlement", {
        intent_id: args.intentId,
        external_tx_ref: args.externalTxRef,
      });
    },

    /**
     * Query aggregate conservation totals for a humanitarian corridor (zero PII).
     * Wasm Service: `"ext-vault"`, GraphQL Query: `corridorConservation`
     */
    getCorridorConservation: async (corridorJurisdiction: string): Promise<CorridorConservation> => {
      assertValidCorridorJurisdiction(corridorJurisdiction);
      const data = await this.query<{ corridorConservation: CorridorConservation }>(
        MUABASE_SERVICES.EXTERNAL_VAULT,
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
      return this.transact(MUABASE_SERVICES.CREDIT_CLEAR, "executeBilateralNetting", args);
    },

    /**
     * Register a licensed bridge-country liquidity desk publishing non-custodial vaults.
     * Wasm Service: `"cred-clear"`, Action: `"registerBridgeLiquidityDesk"`
     */
    registerBridgeLiquidityDesk: async (
      input: RegisterBridgeLiquidityDeskInput,
    ): Promise<{ deskId: number }> => {
      const corridorJurisdiction = assertValidCorridorJurisdiction(input.corridorJurisdiction);
      if (!input.deskLabel || input.deskLabel.trim().length === 0) {
        throw new Error("deskLabel must be non-empty");
      }
      assertPositiveInteger(input.maxPerTxUsdcCents, "maxPerTxUsdcCents");
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

      return this.transact(MUABASE_SERVICES.CREDIT_CLEAR, "registerBridgeLiquidityDesk", {
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
        MUABASE_SERVICES.CREDIT_CLEAR,
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
    createEndowment: async (args: CreateEndowmentInput) => {
      if (!args.label || args.label.trim().length === 0) {
        throw new Error("Endowment label must be non-empty");
      }
      assertPositiveInteger(args.corpusPrincipal, "corpusPrincipal");
      if (args.allowedCategories.length === 0) {
        throw new Error("allowedCategories must contain at least one category");
      }
      if (!Number.isInteger(args.quorumThreshold) || args.quorumThreshold < 2 || args.quorumThreshold > args.boardMembers.length) {
        throw new Error("quorumThreshold must be >= 2 and <= boardMembers.length");
      }
      return this.transact(MUABASE_SERVICES.WAQF_TREASURY, "createEndowment", {
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
    proposeDisbursement: async (args: ProposeDisbursementInput) => {
      assertPositiveInteger(args.amount, "amount");
      return this.transact(MUABASE_SERVICES.WAQF_TREASURY, "proposeDisbursement", {
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
      return this.transact(MUABASE_SERVICES.WAQF_TREASURY, "approveDisbursement", { proposal_id: proposalId });
    },
    getAggregateTreasury: async (): Promise<AggregateTreasury> => {
      const data = await this.query<{ aggregateTreasury: AggregateTreasury }>(
        MUABASE_SERVICES.WAQF_TREASURY,
        `{ aggregateTreasury { endowmentCount totalCorpusLocked totalYieldAvailable totalDisbursed } }`,
      );
      return data.aggregateTreasury;
    },
  };

  public muamalatTahkim = {
    /**
     * Register a scholar-attested commercial contract template (`muamalat::registerTemplate`).
     */
    registerTemplate: async (args: RegisterTemplateInput) => {
      if (!args.standardRef || args.standardRef.trim().length === 0) {
        throw new Error("standardRef must be non-empty");
      }
      return this.transact(MUABASE_SERVICES.MUAMALAT_TAHKIM, "registerTemplate", {
        contract_kind: args.contractKind,
        template_hash: assertValidSha256Commitment(args.templateHash, "templateHash"),
        certificate_hash: assertValidSha256Commitment(args.certificateHash, "certificateHash"),
        standard_ref: args.standardRef,
      });
    },
    createContract: async (args: CreateContractInput) => {
      if (new Set(args.arbitrators).size !== 3) {
        throw new Error("Tahkim panel requires exactly 3 distinct arbitrators");
      }
      if (args.milestoneAmounts.length === 0 || args.milestoneAmounts.some((m) => !Number.isInteger(m) || m <= 0)) {
        throw new Error("milestoneAmounts must contain positive integer amounts");
      }
      return this.transact(MUABASE_SERVICES.MUAMALAT_TAHKIM, "createContract", {
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
    castTahkimVote: async (args: CastTahkimVoteInput) => {
      return this.transact(MUABASE_SERVICES.MUAMALAT_TAHKIM, "castTahkimVote", {
        contract_id: args.contractId,
        ruling_kind: args.rulingKind,
        funder_amount: args.funderAmount ?? 0,
        contractor_amount: args.contractorAmount ?? 0,
      });
    },
  };

  public tokenStream = {
    recordGrossRevenue: async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.TOKEN_STREAM, "recordGrossRevenue", args);
    },
    distributeCoopYield: async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.TOKEN_STREAM, "distributeCoopYield", args);
    },
    allocateYieldStake: async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.TOKEN_STREAM, "allocateYieldStake", args);
    },
    "claimJu'alahReward": async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.TOKEN_STREAM, "claimJu'alahReward", args);
    },
    claimJualahReward: async (args: Record<string, any>) => {
      return this.transact(MUABASE_SERVICES.TOKEN_STREAM, "claimJu'alahReward", args);
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
        return this.transact(MUABASE_SERVICES.TOKENS, "configureAssetPool", {
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
        const result = await this.transact(MUABASE_SERVICES.LEASE_ENGINE, "createLease", {
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
          await this.transact(MUABASE_SERVICES.EXTERNAL_VAULT, "generateSplitIntent", {
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
        return this.transact(MUABASE_SERVICES.LEASE_ENGINE, "confirmCleanReturn", {
          leaseId: parseInt(input.leaseId, 10),
          postConditionEvidenceHash: input.postConditionEvidenceHash,
        });
      },
    },

    claims: {
      submitEvidence: async (input: DamageClaimInput) => {
        return this.transact(MUABASE_SERVICES.CLAIM_COUNCIL, "submitDamageClaim", {
          lease_id: parseInt(input.leaseId, 10),
          respondent: "renter-account",
          pool_id: 1,
          claimed_amount: { value: Math.round(input.claimedAmountUsd * 10000) },
          pre_condition_evidence_hash: input.preConditionEvidenceHash,
          post_condition_evidence_hash: input.postConditionEvidenceHash,
          repair_quote_hash: input.repairQuoteHash,
        });
      },

      voteTriage: async (input: { claimId: string; approve: boolean; payoutUsd: number }) => {
        return this.transact(MUABASE_SERVICES.CLAIM_COUNCIL, "voteDamageClaim", {
          claim_id: parseInt(input.claimId, 10),
          approve: input.approve,
          approved_payout: { value: Math.round(input.payoutUsd * 10000) },
        });
      },
    },

    credit: {
      netInvoices: async (invoice1: string, invoice2: string) => {
        return this.transact(MUABASE_SERVICES.CREDIT_CLEAR, "executeBilateralNetting", {
          invoice_1_id: parseInt(invoice1, 10),
          invoice_2_id: parseInt(invoice2, 10),
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
        return this.transact(MUABASE_SERVICES.TOKENS, "configureAssetPool", {
          config: {
            tokenId: 2001,
            legalWrapperHash: input.legalWrapperHash,
            maxHolders: 499,
            minTrustScore: 6000,
            tangibleRatioBps: input.tangibleRatioBps ?? 10000,
            priorityWindowSec: input.priorityWindowSec ?? 86400,
            custodianAccount: input.custodianAccount || "mizan-custodian",
            custodianFeeBps: 200, // 2% annual management fee
            charitySinkAccount: "waqf-sadaqah",
          },
        });
      },

      reportRevenue: async (input: RevenueReportInput) => {
        return this.transact(MUABASE_SERVICES.TOKEN_STREAM, "recordGrossRevenue", {
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
        assertPositiveInteger(params.portfolioValuationCents, "portfolioValuationCents");
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

        const res = await this.transact(MUABASE_SERVICES.TOKEN_STREAM, "distributeCoopYield", {
          poolId: params.poolId,
          netYieldBps,
          effectivePayoutBps,
          reserveSurplusAddedCents,
          reserveDrawCents,
        });

        return {
          distributionId: String(res.distributionId ?? ""),
          poolId: params.poolId,
          grossYieldBps,
          netYieldBps,
          effectivePayoutBps,
          reserveSurplusAddedCents,
          reserveDrawCents,
          reserveTotalBalanceCents: Number(res.reserveTotalBalanceCents ?? 0),
        };
      },
    },

    shares: {
      invest: async (input: { poolId: number; amountCents: number; rail?: string }) => {
        if (input.rail === "stripe_connect") {
          return await this.transact(MUABASE_SERVICES.EXTERNAL_VAULT, "generateInvestmentIntent", {
            poolId: input.poolId,
            amountCents: input.amountCents,
            railType: "stripe_connect",
            investorAccount: this.account,
          });
        }

        return await this.transact(MUABASE_SERVICES.TOKENS, "purchaseDigitalShares", {
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
        return this.transact(MUABASE_SERVICES.EVALUATIONS, "createJu'alahBounty", {
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
        return this.transact(MUABASE_SERVICES.EVALUATIONS, "resolveJu'alahBounty", {
          marketId: input.marketId,
          winningOutcomeIndex: input.winningOutcomeIndex,
          evidenceHash: input.evidenceHash,
          dualWitnessSignatures: input.dualWitnessSignatures || [],
        });
      },
    },

    vault: {
      allocateStake: async (input: StakeAllocationInput): Promise<MarketPosition> => {
        const result = await this.transact(MUABASE_SERVICES.TOKEN_STREAM, "allocateYieldStake", {
          marketId: input.marketId,
          outcomeIndex: input.outcomeIndex,
          principalAmountUsd: input.principalAmountUsd,
          staker: this.account,
          rail: input.rail || "native_usdc",
        });

        return {
          positionId: String(result.positionId ?? ""),
          marketId: input.marketId,
          account: this.account,
          outcomeIndex: input.outcomeIndex,
          principalAmountUsd: input.principalAmountUsd,
          prizeShares: Math.round(input.principalAmountUsd * 86400),
          allocationTimestamp: Math.floor(Date.now() / 1000),
          status: "Active",
        };
      },

      claim: async (positionId: string): Promise<ClaimRewardResult> => {
        const res = await this.transact(MUABASE_SERVICES.TOKEN_STREAM, "claimJu'alahReward", {
          positionId: parseInt(positionId, 10),
          claimer: this.account,
        });

        return {
          positionId,
          principalRefundUsd: res.principalRefundUsd, // 100% of user principal returned
          yieldPrizeRewardUsd: res.yieldPrizeRewardUsd ?? 0,
          skillPointsEarned: res.skillPointsEarned ?? 0,
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
        return this.transact(MUABASE_SERVICES.TOKENS, "configureAssetPool", {
          config: {
            tokenId: 3001,
            legalWrapperHash: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
            maxHolders: 10000,
            minTrustScore: input.minTrustScore || 6500,
            tangibleRatioBps: input.tangibleRatioBps ?? 10000,
            priorityWindowSec: 86400,
            custodianAccount: "restaurant-registry",
            custodianFeeBps: 0,
            charitySinkAccount: "waqf-sadaqah",
          },
        });
      },

      submitProofOfVisit: async (input: ProofOfVisitInput) => {
        return this.transact(MUABASE_SERVICES.STAGED_TX, "recordAttestation", {
          service: MUABASE_SERVICES.EVALUATIONS,
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
        return this.transact(MUABASE_SERVICES.EVALUATIONS, "recordPairwiseVote", {
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
        const totalAmountCents = Math.round(input.seats * input.pricePerSeatCents);
        const split = computeCommercialSplitCents(totalAmountCents);
        const result = await this.transact(MUABASE_SERVICES.LEASE_ENGINE, "createLease", {
          lessor: "host-chef",
          token_id: input.clubId,
          nft_id: null,
          duration_sec: 14400, // 4-hour dining usufruct
          gross_payment: { value: totalAmountCents * 100 },
          payment_token_id: 1,
          pre_condition_evidence_hash: input.preConditionEvidenceHash || "0x" + "0".repeat(64),
          is_progressive_buyout: false,
          buyout_equity_units: { value: 0 },
          custodian_account: "supper-host",
          custodian_fee_bps: 0,
          charity_sink_account: "waqf-sadaqah",
        });

        if (input.rail === "stripe_connect") {
          await this.transact(MUABASE_SERVICES.EXTERNAL_VAULT, "generateSplitIntent", {
            lease_id: result.leaseId || 401,
            rail_type: "stripe_connect",
            gross_amount_cents: totalAmountCents,
            lender_account: "host-chef",
          });
        }

        return {
          bookingId: String(result.leaseId ?? ""),
          clubId: input.clubId,
          diner: this.account,
          seats: input.seats,
          totalAmountCents,
          status: "Confirmed",
          splits: {
            hostTakeHomeCents: split.ownerAmountCents,
            mutualShieldReserveCents: split.mutualShieldReserveCents,
            platformFeeCents: split.platformFeeCents,
          },
        };
      },

      cancelWithEthicalDeterrence: async (input: BookingCancellationInput) => {
        return this.transact(MUABASE_SERVICES.LEASE_ENGINE, "processLateReturn", {
          lease_id: parseInt(input.bookingId, 10),
          actual_cost: { value: Math.round(input.actualKitchenCostCents * 100) },
          late_deterrent_penalty: { value: Math.round(input.punitiveLateFeeCents * 100) },
          post_condition_evidence_hash: "0x" + "0".repeat(64),
        });
      },
    },

    guilds: {
      createTastemakerGuild: async (input: TastemakerGuildInput) => {
        return this.transact(MUABASE_SERVICES.GUILDS, "createPod", {
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
