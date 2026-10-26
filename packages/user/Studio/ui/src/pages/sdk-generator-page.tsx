import React, { useState } from "react";
import { toast } from "sonner";
import { Code2, Copy, Globe, Layers, Sparkles } from "lucide-react";

import { Badge } from "@shared/shadcn/ui/badge";
import { Button } from "@shared/shadcn/ui/button";
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@shared/shadcn/ui/card";

import { useStudio } from "../context/studio-context";
import { CANONICAL_TERMINOLOGY } from "../lib/studio-data";

export const SdkGeneratorPage: React.FC = () => {
    const { endpoint, locale, toggleLocale, activeAccount, term } = useStudio();
    const [selectedBlueprint, setSelectedBlueprint] = useState<
        "rental" | "dining" | "coop" | "reactive"
    >("rental");

    const codeSnippets: Record<
        "rental" | "dining" | "coop" | "reactive",
        { title: string; code: string }
    > = {
        rental: {
            title: `P2P Rental & Non-Custodial 93%/4%/3% Split (${term("lease")})`,
            code: `import { createMuabaseClient } from "@muabase/headless-sdk";

// Connect directly to psinode Triedent Wasm DB (Zero Backend Middleware)
export const client = createMuabaseClient({
  endpoint: "${endpoint}",
  locale: "${locale}",
  account: "${activeAccount.account}",
});

// 1. Register ${term("asset_pool")} with >= 5100 bps ${term("tangible_ratio")}
await client.tokens.configureAssetPool({
  tokenId: 1042,
  legalWrapperHash: "0x" + "a".repeat(64),
  maxHolders: 99,
  minTrustScore: 7000,
  tangibleRatioBps: 10000,  // 100% tangible asset backing (>= 5100 invariant)
  priorityWindowSec: 86400, // 24h ${term("priority_window")}
  custodianAccount: "custodian",
  custodianFeeBps: 0,
  charitySinkAccount: "waqf-sadaqah",
});

// 2. Execute ${term("lease")} checkout ($180.00 = 1800000 units)
const lease = await client.leaseEngine.createLease({
  lessor: "asset-owner",
  tokenId: 1042,
  durationSec: 4 * 86400,
  grossPayment: { value: 1800000 },
  paymentTokenId: 1,
  preConditionEvidenceHash: "0x" + "c".repeat(64),
  charitySinkAccount: "waqf-sadaqah",
});

// 3. Trigger 0-key ${term("external_vault")} (93% Owner / 4% Reserve / 3% SaaS)
await client.externalVault.generateSplitIntent({
  leaseId: lease.leaseId,
  railType: "stripe_connect",
  grossAmountCents: 18000,
  lenderAccount: "asset-owner",
});`,
        },
        dining: {
            title: `Verified Dining Receipt & Condorcet Tournament (${term("evidence_gate")} + ${term("evals")})`,
            code: `import { createMuabaseClient } from "@muabase/headless-sdk";

export const client = createMuabaseClient({
  endpoint: "${endpoint}",
  locale: "${locale}",
  account: "${activeAccount.account}",
});

// 1. Record ${term("evidence_gate")} attestation (SHA-256 of POS receipt)
await client.stagedTx.recordAttestation({
  service: "evaluation",
  entityId: 3001,
  evidenceHash: "0x" + "b".repeat(64),
  attester: "${activeAccount.account}",
  witnessQuorum: 2, // Dual-witness invariant
  metadata: {
    ratingBps: 9400, // 9.40 / 10.0
    timestamp: Math.floor(Date.now() / 1000),
  },
});

// 2. Submit pairwise ballot into Wasm Schulze Condorcet tournament (${term("evals")})
await client.evaluations.recordPairwiseVote({
  winnerId: 3001,
  loserId: 2042,
  category: "food",
  voter: "${activeAccount.account}",
});`,
        },
        coop: {
            title: `Real Estate & Solar Co-Op Yield Smoothing (${term("endowment_policy")})`,
            code: `import { createMuabaseClient } from "@muabase/headless-sdk";

export const client = createMuabaseClient({
  endpoint: "${endpoint}",
  locale: "${locale}",
  account: "${activeAccount.account}",
});

// 1. Report verified solar PPA or workforce housing revenue
await client.coop.revenue.reportAndDistribute({
  poolId: 1,
  source: "commercial_solar_ppa",
  grossAmountCents: 4250000,
  operatingExpensesCents: 650000,
  evidenceHash: "0x" + "e".repeat(64),
});

// 2. Execute zero-fee B2B invoice netting (${term("credit_clear")})
await client.creditClear.executeBilateralNetting({
  invoice1Id: 501,
  invoice2Id: 502,
});`,
        },
        reactive: {
            title: "SpacetimeDB-Style Reactive Table Hook (useMuabaseTable)",
            code: `import React from "react";
import { createMuabaseClient, useMuabaseTable } from "@muabase/headless-sdk";

const client = createMuabaseClient({
  endpoint: "${endpoint}",
  locale: "${locale}",
  account: "${activeAccount.account}",
});

export function LiveLeasesFeed() {
  const { data: leases, loading } = useMuabaseTable({
    client,
    service: "lease-eng",
    table: "UsufructLeaseTable",
    query: "{ usufructLeaseTable { leaseId lessor lessee grossPayment status } }",
    pollIntervalMs: 2000,
  });

  if (loading) return <div>Syncing Triedent state...</div>;

  return (
    <ul>
      {leases.map((lease) => (
        <li key={lease.leaseId}>
          Lease #{lease.leaseId}: {lease.lessor} -> {lease.lessee} ({lease.status})
        </li>
      ))}
    </ul>
  );
}`,
        },
    };

    const currentSnippet = codeSnippets[selectedBlueprint];

    return (
        <div className="mx-auto max-w-7xl space-y-6">
            <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-center">
                <div>
                    <h1 className="flex items-center gap-2 text-2xl font-bold tracking-tight">
                        <Code2 className="h-6 w-6 text-emerald-400" />
                        Headless SDK Generator &amp; Dual-Locale Dictionary
                    </h1>
                    <p className="text-muted-foreground text-xs">
                        Generate zero-middleware{" "}
                        <code className="text-foreground">
                            @muabase/headless-sdk
                        </code>{" "}
                        TypeScript snippets and inspect the 17 canonical{" "}
                        <code className="text-foreground">Branding</code>{" "}
                        terminology mappings.
                    </p>
                </div>
                <Button
                    variant="outline"
                    size="sm"
                    onClick={toggleLocale}
                    className="border-amber-500/30 bg-amber-500/10 text-xs text-amber-300"
                >
                    <Globe className="mr-1.5 h-3.5 w-3.5" />
                    Switch Active Locale ({locale})
                </Button>
            </div>

            {/* SDK Code Generator Card */}
            <Card>
                <CardHeader className="pb-3">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                        <div>
                            <CardTitle className="flex items-center gap-2 text-base">
                                <Sparkles className="h-4 w-4 text-emerald-400" />
                                {currentSnippet.title}
                            </CardTitle>
                            <CardDescription className="text-xs">
                                Configured for endpoint{" "}
                                <code>{endpoint}</code>, account{" "}
                                <code>@{activeAccount.account}</code>, and
                                locale <code>&quot;{locale}&quot;</code>.
                            </CardDescription>
                        </div>

                        <div className="flex flex-wrap items-center gap-1.5">
                            {(
                                [
                                    ["rental", "P2P Rental"],
                                    ["dining", "Dining & Condorcet"],
                                    ["coop", "Solar Co-Op & Netting"],
                                    ["reactive", "useMuabaseTable Hook"],
                                ] as const
                            ).map(([key, label]) => (
                                <button
                                    key={key}
                                    type="button"
                                    onClick={() => setSelectedBlueprint(key)}
                                    className={`rounded-md px-2.5 py-1 text-xs font-medium transition-colors ${
                                        selectedBlueprint === key
                                            ? "bg-emerald-600 text-white"
                                            : "bg-muted text-muted-foreground hover:text-foreground"
                                    }`}
                                >
                                    {label}
                                </button>
                            ))}
                        </div>
                    </div>
                </CardHeader>
                <CardContent className="space-y-3">
                    <div className="relative">
                        <pre className="border-border bg-zinc-950 overflow-x-auto rounded-lg border p-4 font-mono text-xs leading-relaxed text-emerald-300">
                            {currentSnippet.code}
                        </pre>
                        <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => {
                                navigator.clipboard.writeText(
                                    currentSnippet.code,
                                );
                                toast.success(
                                    "Copied TypeScript SDK code to clipboard",
                                );
                            }}
                            className="absolute top-3 right-3 h-7 text-xs"
                        >
                            <Copy className="mr-1.5 h-3.5 w-3.5" />
                            Copy Code
                        </Button>
                    </div>
                </CardContent>
            </Card>

            {/* 17-Key Canonical Terminology Disambiguation Table */}
            <Card>
                <CardHeader className="pb-3">
                    <div className="flex items-center justify-between">
                        <div>
                            <CardTitle className="flex items-center gap-2 text-base">
                                <Layers className="h-4 w-4 text-amber-400" />
                                On-Chain Dual-Locale Terminology Dictionary (
                                <code>packages/user/Branding</code>)
                            </CardTitle>
                            <CardDescription className="text-xs">
                                Institutional and Muamalat terms are
                                presentation labels; Wasm service slugs always
                                remain canonical.
                            </CardDescription>
                        </div>
                        <Badge variant="outline" className="font-mono text-xs">
                            17 Canonical Keys
                        </Badge>
                    </div>
                </CardHeader>
                <CardContent>
                    <div
                        tabIndex={0}
                        role="region"
                        aria-label="Dual-Locale Terminology Dictionary"
                        className="studio-table-wrapper border-border rounded-lg border"
                    >
                        <table className="text-left text-xs">
                            <thead className="bg-muted/90 text-muted-foreground border-b">
                                <tr>
                                    <th
                                        scope="col"
                                        className="px-3.5 py-2.5 font-mono text-[11px]"
                                    >
                                        Canonical Key
                                    </th>
                                    <th
                                        scope="col"
                                        className="px-3.5 py-2.5 font-mono text-[11px]"
                                    >
                                        Institutional Locale (&quot;institutional&quot;)
                                    </th>
                                    <th
                                        scope="col"
                                        className="px-3.5 py-2.5 font-mono text-[11px]"
                                    >
                                        Jurisprudential Locale (&quot;muamalat&quot;)
                                    </th>
                                    <th
                                        scope="col"
                                        className="px-3.5 py-2.5 font-mono text-[11px]"
                                    >
                                        Wasm Service
                                    </th>
                                </tr>
                            </thead>
                            <tbody className="divide-border divide-y">
                                {CANONICAL_TERMINOLOGY.map((item) => (
                                    <tr
                                        key={item.key}
                                        className="hover:bg-muted/30"
                                    >
                                        <th
                                            scope="row"
                                            className="border-border border-r px-3.5 py-2 font-mono font-semibold text-emerald-400"
                                        >
                                            {item.key}
                                        </th>
                                        <td
                                            className={`px-3.5 py-2 ${
                                                locale === "institutional"
                                                    ? "font-semibold text-white"
                                                    : "text-muted-foreground"
                                            }`}
                                        >
                                            {item.institutional}
                                        </td>
                                        <td
                                            className={`px-3.5 py-2 ${
                                                locale === "muamalat"
                                                    ? "font-semibold text-amber-300"
                                                    : "text-muted-foreground"
                                            }`}
                                        >
                                            {item.muamalat}
                                        </td>
                                        <td className="px-3.5 py-2 font-mono">
                                            <Badge
                                                variant="outline"
                                                className="font-mono text-[10px]"
                                            >
                                                &quot;{item.service}&quot;
                                            </Badge>
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                </CardContent>
            </Card>
        </div>
    );
};
