import type { TerminologyLocale } from "@shared/headless-sdk/src/types";

export interface TerminologyMapping {
    key: string;
    institutional: string;
    muamalat: string;
    service: string;
    description: string;
}

export const CANONICAL_TERMINOLOGY: TerminologyMapping[] = [
    {
        key: "asset_pool",
        institutional: "RWA Asset Series / Series LLC",
        muamalat: "Sukuk al-Qirad / Musharakah",
        service: "tokens",
        description:
            "Fractional co-ownership of real physical assets with shared revenue and par-value floor (>= 51%).",
    },
    {
        key: "lease",
        institutional: "Fixed Usufruct Lease / P2P Rental",
        muamalat: "Ijarah al-A‘yan wa al-Manafi‘",
        service: "lease-eng",
        description:
            "Separation of permanent asset ownership from temporary right-of-use with 93%/4%/3% splits.",
    },
    {
        key: "equity_lease",
        institutional: "Progressive Lease-to-Own / Equity Lease",
        muamalat: "Musharakah Mutanaqisah",
        service: "lease-eng",
        description:
            "Gradual transfer of fractional asset equity alongside decreasing rental installments.",
    },
    {
        key: "mutual_shield",
        institutional: "Mutual Protection Reserve Pool",
        muamalat: "Takaful (Tabarru‘ & Ta‘awun)",
        service: "tokenstream",
        description:
            "Member-owned protection reserve (4% split) based on mutual risk sharing with surplus rebates.",
    },
    {
        key: "claim_council",
        institutional: "Peer Triage Committee",
        muamalat: "Majlis al-Tahkim & Shura",
        service: "claimcouncil",
        description:
            "3-peer rapid damage arbitration committee verifying cryptographic evidence digests.",
    },
    {
        key: "evals",
        institutional: "Objective Bounty & Claim Ranking",
        muamalat: "Ju‘alah Halaqa",
        service: "evals",
        description:
            "Wasm Schulze Condorcet pairwise tournaments for verified work, restaurants, and claims (never people).",
    },
    {
        key: "endowment_policy",
        institutional: "Trust Principal Preservation Lock",
        muamalat: "Hifz Asl al-Waqf",
        service: "tokenstream",
        description:
            "Strict mathematical lock preventing outward capital consumption transfers from principal accounts.",
    },
    {
        key: "priority_window",
        institutional: "Co-Owner Right of First Refusal",
        muamalat: "Haqq al-Shuf‘ah",
        service: "tokens",
        description:
            "Mandatory 24-hour exclusive pre-emption window for existing fractional co-owners.",
    },
    {
        key: "charity_sink",
        institutional: "Public Endowment Charity Sink",
        muamalat: "Masraf al-Sadaqah",
        service: "lease-eng",
        description:
            "100% of punitive late penalties and cancellation surcharges route to 'waqf-sadaqah'.",
    },
    {
        key: "credit_clear",
        institutional: "Multilateral Invoice Netting",
        muamalat: "Muqassah & Wajahah-Hawalah",
        service: "creditclear",
        description:
            "Zero-fee algorithmic B2B debt set-off between reciprocal trading partners.",
    },
    {
        key: "liquidity_bridge",
        institutional: "Earned-Respect Liquidity Bridge",
        muamalat: "Wajahah & Hawalah",
        service: "creditclear",
        description:
            "Debt novation and liquidity routing backed by verified commercial reputation.",
    },
    {
        key: "trust_score",
        institutional: "Reliability & Audit Score",
        muamalat: "‘Adalah (Probity / Reliability)",
        service: "credentials",
        description:
            "Soulbound, non-transferable track record in basis points (0–10,000 bps).",
    },
    {
        key: "domain_badge",
        institutional: "Domain Competence Credential",
        muamalat: "Ikhtisas (Domain Specialization)",
        service: "credentials",
        description:
            "Soulbound specialization credential (e.g., master_appraiser, sommelier_l3, solar_auditor).",
    },
    {
        key: "tangible_ratio",
        institutional: "Tangible Asset Backing Ratio (>= 51%)",
        muamalat: "Nisbat al-‘Ayn al-Haqiqiyyah",
        service: "tokens",
        description:
            "Hardcoded >= 5100 bps tangible physical asset backing floor barring naked debt speculation.",
    },
    {
        key: "evidence_gate",
        institutional: "Dual-Witness Evidentiary Gate",
        muamalat: "Bayyinah (Dual Proof / Attestation)",
        service: "staged-tx",
        description:
            "2-witness quorum verification of SHA-256 Ricardian photo/receipt digests (H(M)).",
    },
    {
        key: "external_vault",
        institutional: "Non-Custodial Multi-Chain Vault & Splits",
        muamalat: "Amanah Ghayr Hiyaziyyah (Trustee Escrow)",
        service: "ext-vault",
        description:
            "0-key client-side execution via Stripe Connect, Bitcoin Taproot Miniscript, and EVM Safe.",
    },
    {
        key: "guild_pod",
        institutional: "Cooperative Pod & Review Circle",
        muamalat: "Halaqat al-Tijarah / al-Shura",
        service: "guilds",
        description:
            "Autonomous 5–7 peer governance pods operating consensus without managerial capture.",
    },
];

export function formatTerm(
    canonicalKey: string,
    locale: TerminologyLocale,
): string {
    const found = CANONICAL_TERMINOLOGY.find((t) => t.key === canonicalKey);
    if (!found) return canonicalKey;
    return locale === "muamalat" ? found.muamalat : found.institutional;
}

export interface StudioAccountProfile {
    account: string;
    role: string;
    trustScoreBps: number;
    domainBadges: string[];
}

export const STUDIO_ACCOUNTS: StudioAccountProfile[] = [
    {
        account: "alice",
        role: "Lead Builder & Verified Member",
        trustScoreBps: 9450,
        domainBadges: ["rwa_appraiser", "culinary_inspector"],
    },
    {
        account: "asset-owner",
        role: "RWA Lessor & Cooperative Host",
        trustScoreBps: 9800,
        domainBadges: ["luxury_custodian", "solar_operator"],
    },
    {
        account: "chef-omar",
        role: "Supper Club Host & Tastemaker",
        trustScoreBps: 9620,
        domainBadges: ["michelin_alumni", "sommelier_l3"],
    },
    {
        account: "inspector-1",
        role: "Claim Council Peer Arbitrator",
        trustScoreBps: 9910,
        domainBadges: ["claim_arbitrator", "structural_engineer"],
    },
    {
        account: "waqf-sadaqah",
        role: "Public Endowment Charity Sink Vault",
        trustScoreBps: 10000,
        domainBadges: ["endowment_vault"],
    },
];

export interface StudioTableDefinition {
    id: string;
    tableName: string;
    service: string;
    canonicalKey: string;
    primaryKey: string;
    description: string;
    graphqlQuery: string;
    columns: Array<{
        key: string;
        label: string;
        type: "string" | "number" | "bps" | "usd4" | "cents" | "hash" | "badge" | "boolean";
    }>;
}

export const STUDIO_TABLES: StudioTableDefinition[] = [
    {
        id: "AssetPoolTable",
        tableName: "AssetPoolTable",
        service: "tokens",
        canonicalKey: "asset_pool",
        primaryKey: "tokenId",
        description:
            "Registered RWA asset series with >= 5100 bps tangible floor, co-owner ROFR windows, and charity sink routing.",
        graphqlQuery: `{
  assetPoolTable {
    tokenId
    title
    tangibleRatioBps
    minTrustScore
    priorityWindowSec
    maxHolders
    charitySinkAccount
    legalWrapperHash
  }
}`,
        columns: [
            { key: "tokenId", label: "Token ID", type: "number" },
            { key: "title", label: "Series Title", type: "string" },
            { key: "tangibleRatioBps", label: "Tangible Ratio", type: "bps" },
            { key: "minTrustScore", label: "Min Trust Score", type: "bps" },
            { key: "priorityWindowSec", label: "ROFR Window (s)", type: "number" },
            { key: "charitySinkAccount", label: "Charity Sink", type: "badge" },
            { key: "legalWrapperHash", label: "Legal Digest H(M)", type: "hash" },
        ],
    },
    {
        id: "UsufructLeaseTable",
        tableName: "UsufructLeaseTable",
        service: "lease-eng",
        canonicalKey: "lease",
        primaryKey: "leaseId",
        description:
            "Active and completed P2P rentals, supper club bookings, and progressive equity leases (93% Owner / 4% Mutual Shield / 3% SaaS).",
        graphqlQuery: `{
  usufructLeaseTable {
    leaseId
    lessor
    lessee
    tokenId
    durationSec
    grossPayment
    status
    isProgressiveBuyout
    charitySinkAccount
    preConditionEvidenceHash
  }
}`,
        columns: [
            { key: "leaseId", label: "Lease ID", type: "number" },
            { key: "lessor", label: "Lessor", type: "badge" },
            { key: "lessee", label: "Lessee", type: "badge" },
            { key: "tokenId", label: "Token ID", type: "number" },
            { key: "grossPayment", label: "Gross Payment", type: "usd4" },
            { key: "status", label: "Status", type: "badge" },
            { key: "preConditionEvidenceHash", label: "Pre-Condition H(M)", type: "hash" },
        ],
    },
    {
        id: "EvidenceAttestationTable",
        tableName: "EvidenceAttestationTable",
        service: "staged-tx",
        canonicalKey: "evidence_gate",
        primaryKey: "attestationId",
        description:
            "Dual-witness Ricardian evidence digests (SHA-256) gating escrow releases, damage claims, and verified dining reviews.",
        graphqlQuery: `{
  evidenceAttestationTable {
    attestationId
    service
    entityId
    attester
    witnessQuorum
    ratingBps
    evidenceHash
    verified
  }
}`,
        columns: [
            { key: "attestationId", label: "Attestation ID", type: "number" },
            { key: "service", label: "Target Service", type: "badge" },
            { key: "entityId", label: "Entity ID", type: "number" },
            { key: "attester", label: "Attester", type: "badge" },
            { key: "witnessQuorum", label: "Witness Quorum", type: "number" },
            { key: "ratingBps", label: "Score", type: "bps" },
            { key: "evidenceHash", label: "SHA-256 Digest", type: "hash" },
        ],
    },
    {
        id: "CondorcetRankingTable",
        tableName: "CondorcetRankingTable",
        service: "evals",
        canonicalKey: "evals",
        primaryKey: "entityId",
        description:
            "Wasm Schulze Condorcet pairwise tournament standings for verified deliverables, restaurants, and bounties (people are never ranked).",
        graphqlQuery: `{
  condorcetRankingTable {
    entityId
    entityName
    category
    schulzeRank
    pairwiseWins
    pairwiseLosses
    bountyPoolUsd4
  }
}`,
        columns: [
            { key: "entityId", label: "Entity ID", type: "number" },
            { key: "entityName", label: "Deliverable / Venue", type: "string" },
            { key: "category", label: "Category", type: "badge" },
            { key: "schulzeRank", label: "Condorcet Rank", type: "number" },
            { key: "pairwiseWins", label: "Pairwise Wins", type: "number" },
            { key: "pairwiseLosses", label: "Pairwise Losses", type: "number" },
            { key: "bountyPoolUsd4", label: "Bounty Pool", type: "usd4" },
        ],
    },
    {
        id: "DamageClaimTable",
        tableName: "DamageClaimTable",
        service: "claimcouncil",
        canonicalKey: "claim_council",
        primaryKey: "claimId",
        description:
            "3-peer rapid triage committee records for MutualShield protection disbursements.",
        graphqlQuery: `{
  damageClaimTable {
    claimId
    leaseId
    respondent
    claimedAmount
    votesApprove
    votesReject
    status
    repairQuoteHash
  }
}`,
        columns: [
            { key: "claimId", label: "Claim ID", type: "number" },
            { key: "leaseId", label: "Lease ID", type: "number" },
            { key: "respondent", label: "Respondent", type: "badge" },
            { key: "claimedAmount", label: "Claimed Amount", type: "usd4" },
            { key: "votesApprove", label: "Approvals (of 3)", type: "number" },
            { key: "status", label: "Triage Status", type: "badge" },
            { key: "repairQuoteHash", label: "Repair Quote H(M)", type: "hash" },
        ],
    },
    {
        id: "SplitIntentTable",
        tableName: "SplitIntentTable",
        service: "ext-vault",
        canonicalKey: "external_vault",
        primaryKey: "intentId",
        description:
            "0-Key Non-Custodial multi-party payment splits across Stripe Connect, Bitcoin Taproot Miniscript, and EVM Safe.",
        graphqlQuery: `{
  splitIntentTable {
    intentId
    leaseId
    railType
    grossAmountCents
    owner93Cents
    mutualShield4Cents
    saasOperator3Cents
    lenderAccount
  }
}`,
        columns: [
            { key: "intentId", label: "Intent ID", type: "number" },
            { key: "leaseId", label: "Lease ID", type: "number" },
            { key: "railType", label: "Settlement Rail", type: "badge" },
            { key: "grossAmountCents", label: "Gross Total", type: "cents" },
            { key: "owner93Cents", label: "93% Owner Share", type: "cents" },
            { key: "mutualShield4Cents", label: "4% Mutual Shield", type: "cents" },
            { key: "saasOperator3Cents", label: "3% SaaS Fee", type: "cents" },
        ],
    },
    {
        id: "InvoiceNettingTable",
        tableName: "InvoiceNettingTable",
        service: "creditclear",
        canonicalKey: "credit_clear",
        primaryKey: "nettingId",
        description:
            "Zero-fee bilateral and cyclic B2B trade credit clearing and Wajahah liquidity routing.",
        graphqlQuery: `{
  invoiceNettingTable {
    nettingId
    invoice1Id
    invoice2Id
    partyA
    partyB
    clearedAmountUsd4
    status
  }
}`,
        columns: [
            { key: "nettingId", label: "Netting ID", type: "number" },
            { key: "invoice1Id", label: "Invoice #1", type: "number" },
            { key: "invoice2Id", label: "Invoice #2", type: "number" },
            { key: "partyA", label: "Counterparty A", type: "badge" },
            { key: "partyB", label: "Counterparty B", type: "badge" },
            { key: "clearedAmountUsd4", label: "Cleared Obligations", type: "usd4" },
            { key: "status", label: "Netting Status", type: "badge" },
        ],
    },
    {
        id: "EndowmentYieldTable",
        tableName: "EndowmentYieldTable",
        service: "tokenstream",
        canonicalKey: "endowment_policy",
        primaryKey: "poolId",
        description:
            "Cooperative real estate & solar pools with mathematical principal preservation locks and 8–12% smoothed yield bands.",
        graphqlQuery: `{
  endowmentYieldTable {
    poolId
    poolName
    principalLockedUsd4
    grossYieldBps
    effectivePayoutBps
    reserveBalanceUsd4
    principalGuardLocked
  }
}`,
        columns: [
            { key: "poolId", label: "Pool ID", type: "number" },
            { key: "poolName", label: "Cooperative Pool", type: "string" },
            { key: "principalLockedUsd4", label: "Locked Principal", type: "usd4" },
            { key: "grossYieldBps", label: "Gross Yield", type: "bps" },
            { key: "effectivePayoutBps", label: "Smoothed Payout", type: "bps" },
            { key: "reserveBalanceUsd4", label: "Stabilization Reserve", type: "usd4" },
            { key: "principalGuardLocked", label: "Principal Guard", type: "boolean" },
        ],
    },
];

export const INITIAL_TABLE_ROWS: Record<string, Record<string, any>[]> = {
    AssetPoolTable: [
        {
            tokenId: 1042,
            title: "Chanel Classic Flap Archive Series #1042",
            tangibleRatioBps: 10000,
            minTrustScore: 7500,
            priorityWindowSec: 86400,
            maxHolders: 99,
            custodianAccount: "custodian",
            charitySinkAccount: "waqf-sadaqah",
            legalWrapperHash:
                "0x8a4f2c91e03b77d215a9e41028c3f6b7d8e9a0123456789abcdef0123456789a",
        },
        {
            tokenId: 2010,
            title: "Brooklyn Community Solar Microgrid PPA #2010",
            tangibleRatioBps: 9400,
            minTrustScore: 7000,
            priorityWindowSec: 86400,
            maxHolders: 250,
            custodianAccount: "asset-owner",
            charitySinkAccount: "waqf-sadaqah",
            legalWrapperHash:
                "0x3f91b7c4d2e8016a54f209c8e1174a6b90d3c2e1f4a5b6c7d8e9f0a1b2c3d4e5",
        },
        {
            tokenId: 3005,
            title: "Leica M11 Cinema Lens Co-Op Kit #3005",
            tangibleRatioBps: 10000,
            minTrustScore: 8000,
            priorityWindowSec: 172800,
            maxHolders: 24,
            custodianAccount: "asset-owner",
            charitySinkAccount: "waqf-sadaqah",
            legalWrapperHash:
                "0x7c1d5e9a2b4f803165e2d9c4b7a0f1234567890abcdef1234567890abcdef123",
        },
    ],
    UsufructLeaseTable: [
        {
            leaseId: 101,
            lessor: "asset-owner",
            lessee: "alice",
            tokenId: 1042,
            durationSec: 345600,
            grossPayment: 1800000, // $180.00 in precision 4
            status: "Active",
            isProgressiveBuyout: false,
            charitySinkAccount: "waqf-sadaqah",
            preConditionEvidenceHash:
                "0x4b9e2d1c8a7f6054321fedcba9876543210fedcba9876543210fedcba9876543",
        },
        {
            leaseId: 102,
            lessor: "chef-omar",
            lessee: "alice",
            tokenId: 3001,
            durationSec: 14400,
            grossPayment: 2400000, // $240.00 supper club table
            status: "ReturnedClean",
            isProgressiveBuyout: false,
            charitySinkAccount: "waqf-sadaqah",
            preConditionEvidenceHash:
                "0x9d1a3c5e7f0b2468ace13579bdf02468ace13579bdf02468ace13579bdf02468",
        },
        {
            leaseId: 103,
            lessor: "asset-owner",
            lessee: "inspector-1",
            tokenId: 2010,
            durationSec: 2592000,
            grossPayment: 14500000, // $1,450.00 progressive lease
            status: "Active",
            isProgressiveBuyout: true,
            charitySinkAccount: "waqf-sadaqah",
            preConditionEvidenceHash:
                "0x1f2e3d4c5b6a79801f2e3d4c5b6a79801f2e3d4c5b6a79801f2e3d4c5b6a7980",
        },
    ],
    EvidenceAttestationTable: [
        {
            attestationId: 501,
            service: "evaluation",
            entityId: 3001,
            attester: "alice",
            witnessQuorum: 2,
            ratingBps: 9400,
            evidenceHash:
                "0x8f2d4a6c1e9b03758f2d4a6c1e9b03758f2d4a6c1e9b03758f2d4a6c1e9b0375",
            verified: true,
        },
        {
            attestationId: 502,
            service: "lease-eng",
            entityId: 101,
            attester: "inspector-1",
            witnessQuorum: 2,
            ratingBps: 10000,
            evidenceHash:
                "0x4b9e2d1c8a7f6054321fedcba9876543210fedcba9876543210fedcba9876543",
            verified: true,
        },
    ],
    CondorcetRankingTable: [
        {
            entityId: 3001,
            entityName: "Sufra Woodfire Supper Club (SoHo)",
            category: "food",
            schulzeRank: 1,
            pairwiseWins: 42,
            pairwiseLosses: 4,
            bountyPoolUsd4: 12000000,
        },
        {
            entityId: 2042,
            entityName: "Al-Andalus Tasting Room (TriBeCa)",
            category: "food",
            schulzeRank: 2,
            pairwiseWins: 35,
            pairwiseLosses: 11,
            bountyPoolUsd4: 8500000,
        },
        {
            entityId: 4019,
            entityName: "Zero-Copy Triedent B-Tree Compaction Patch",
            category: "bounty",
            schulzeRank: 1,
            pairwiseWins: 19,
            pairwiseLosses: 1,
            bountyPoolUsd4: 50000000,
        },
    ],
    DamageClaimTable: [
        {
            claimId: 701,
            leaseId: 101,
            respondent: "alice",
            claimedAmount: 650000, // $65.00
            votesApprove: 3,
            votesReject: 0,
            status: "Disbursed",
            repairQuoteHash:
                "0x5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b",
        },
    ],
    SplitIntentTable: [
        {
            intentId: 901,
            leaseId: 101,
            railType: "stripe_connect",
            grossAmountCents: 18000,
            owner93Cents: 16740,
            mutualShield4Cents: 720,
            saasOperator3Cents: 540,
            lenderAccount: "asset-owner",
        },
        {
            intentId: 902,
            leaseId: 102,
            railType: "bitcoin_taproot",
            grossAmountCents: 24000,
            owner93Cents: 22320,
            mutualShield4Cents: 960,
            saasOperator3Cents: 720,
            lenderAccount: "chef-omar",
        },
    ],
    InvoiceNettingTable: [
        {
            nettingId: 801,
            invoice1Id: 410,
            invoice2Id: 411,
            partyA: "asset-owner",
            partyB: "chef-omar",
            clearedAmountUsd4: 42000000, // $4,200.00 netted with $0 fee
            status: "ClearedZeroFee",
        },
    ],
    EndowmentYieldTable: [
        {
            poolId: 1,
            poolName: "MizanVest Workforce Housing & Solar Trust I",
            principalLockedUsd4: 2500000000, // $250,000.00
            grossYieldBps: 1140, // 11.40%
            effectivePayoutBps: 1025, // 10.25% smoothed payout
            reserveBalanceUsd4: 112500000, // $11,250.00 stabilization buffer
            principalGuardLocked: true,
        },
    ],
};
