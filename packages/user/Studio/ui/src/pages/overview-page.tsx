import React from "react";
import { useNavigate } from "react-router-dom";
import { toast } from "sonner";
import {
    ArrowRight,
    CheckCircle2,
    Code2,
    Cpu,
    Database,
    Globe,
    Layers,
    Play,
    ShieldCheck,
    Sparkles,
    Zap,
} from "lucide-react";

import { Badge } from "@shared/shadcn/ui/badge";
import { Button } from "@shared/shadcn/ui/button";
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@shared/shadcn/ui/card";

import { useStudio, computeSha256Hex } from "../context/studio-context";
import { STUDIO_TABLES } from "../lib/studio-data";

export const OverviewPage: React.FC = () => {
    const navigate = useNavigate();
    const {
        locale,
        toggleLocale,
        term,
        tables,
        executeStudioAction,
        activeAccount,
    } = useStudio();

    const totalRows = Object.values(tables).reduce(
        (acc, r) => acc + r.length,
        0,
    );

    const runVerticalWorkflow = async (
        vertical: "pickle" | "beli" | "mizan" | "irfan",
    ) => {
        if (vertical === "pickle") {
            const hash = await computeSha256Hex(
                `pickle-precondition-${Date.now()}`,
            );
            const res = await executeStudioAction("lease-eng", "createLease", {
                lessor: "asset-owner",
                tokenId: 1042,
                durationSec: 4 * 86400,
                grossPayment: { value: 1800000 },
                paymentTokenId: 1,
                preConditionEvidenceHash: hash,
                charitySinkAccount: "waqf-sadaqah",
            });
            await executeStudioAction("ext-vault", "generateSplitIntent", {
                leaseId: 104,
                railType: "stripe_connect",
                grossAmountCents: 18000,
                lenderAccount: "asset-owner",
            });
            toast.success("Pickle P2P Rental Executed", {
                description: res.message,
            });
        } else if (vertical === "beli") {
            const hash = await computeSha256Hex(`pos-receipt-${Date.now()}`);
            await executeStudioAction("staged-tx", "recordAttestation", {
                service: "evaluation",
                entityId: 3001,
                evidenceHash: hash,
                attester: activeAccount.account,
                witnessQuorum: 2,
                metadata: { ratingBps: 9550 },
            });
            const res = await executeStudioAction(
                "evals",
                "recordPairwiseVote",
                {
                    winnerId: 3001,
                    loserId: 2042,
                    category: "food",
                    voter: activeAccount.account,
                },
            );
            toast.success("Verified Dining Proof & Condorcet Vote Recorded", {
                description: res.message,
            });
        } else if (vertical === "mizan") {
            const hash = await computeSha256Hex(`solar-series-${Date.now()}`);
            const res = await executeStudioAction(
                "tokens",
                "configureAssetPool",
                {
                    config: {
                        tokenId: Math.floor(2100 + Math.random() * 800),
                        title: "Queens Workforce Solar Co-Op Series",
                        legalWrapperHash: hash,
                        maxHolders: 150,
                        minTrustScore: 7000,
                        tangibleRatioBps: 10000,
                        priorityWindowSec: 86400,
                        custodianAccount: "asset-owner",
                        custodianFeeBps: 0,
                        charitySinkAccount: "waqf-sadaqah",
                    },
                },
            );
            toast.success("MizanVest Cooperative Asset Pool Configured", {
                description: res.message,
            });
        } else {
            const res = await executeStudioAction(
                "creditclear",
                "executeBilateralNetting",
                {
                    invoice1Id: 601,
                    invoice2Id: 602,
                    clearedAmountUsd4: 35000000,
                },
            );
            toast.success("Zero-Fee Multilateral Invoice Netting Executed", {
                description: res.message,
            });
        }
    };

    const quickstarts = [
        {
            id: "pickle" as const,
            title: "P2P Luxury Rental (Pickle Competitor)",
            badge: "lease + ext-vault",
            description:
                "Execute a 4-day rental checkout with autonomous 93% owner / 4% mutual shield / 3% SaaS split and zero-penalty charity sink routing.",
            cta: "Simulate Rental Checkout",
        },
        {
            id: "beli" as const,
            title: "Verified Restaurant Curation (Beli Competitor)",
            badge: "staged-tx + evals",
            description:
                "Hash a POS dining receipt with SHA-256 (2-witness Bayyinah gate) and submit a pairwise ballot to the Wasm Schulze Condorcet tournament.",
            cta: "Simulate Receipt & Ballot",
        },
        {
            id: "mizan" as const,
            title: "Real Estate & Solar Co-Op (MizanVest)",
            badge: "tokens + tokenstream",
            description:
                "Configure a 100% tangible asset pool (>= 5100 bps invariant) with 24h co-owner ROFR and endowment principal preservation lock.",
            cta: "Register Co-Op Asset Series",
        },
        {
            id: "irfan" as const,
            title: "B2B Trade Clearinghouse (CreditClear)",
            badge: "creditclear",
            description:
                "Extinguish reciprocal B2B trade invoices algorithmically with $0 bank wire fees or interest-bearing factoring.",
            cta: "Execute Bilateral Netting",
        },
    ];

    return (
        <div className="mx-auto max-w-7xl space-y-6">
            {/* Hero Banner */}
            <div className="border-border relative overflow-hidden rounded-xl border bg-gradient-to-br from-emerald-950/40 via-zinc-900/90 to-zinc-950 p-6">
                <div className="flex flex-col justify-between gap-4 lg:flex-row lg:items-center">
                    <div className="space-y-2">
                        <div className="flex flex-wrap items-center gap-2">
                            <Badge className="bg-emerald-500/20 text-emerald-300 hover:bg-emerald-500/30">
                                SpacetimeDB Speed + Supabase Ergonomics
                            </Badge>
                            <Badge variant="outline" className="font-mono text-xs">
                                Active Locale: {locale}
                            </Badge>
                        </div>
                        <h1 className="text-2xl font-bold tracking-tight sm:text-3xl">
                            Muabase Studio Console
                        </h1>
                        <p className="text-muted-foreground max-w-3xl text-sm">
                            Inspect memory-mapped <code className="text-foreground">Triedent</code>{" "}
                            tables in real time, dispatch canonical WebAssembly reducers, verify
                            commercial invariants (<code className="text-emerald-400">93%/4%/3%</code>{" "}
                            splits, <code className="text-emerald-400">tangibleRatioBps &ge; 5100</code>
                            , 2-witness <code className="text-emerald-400">evidence_gate</code>),
                            and generate zero-middleware TypeScript SDK code.
                        </p>
                    </div>
                    <div className="flex flex-wrap items-center gap-2.5">
                        <Button
                            onClick={() => navigate("/tables")}
                            className="bg-emerald-600 text-white hover:bg-emerald-500"
                        >
                            <Database className="mr-1.5 h-4 w-4" />
                            Explore Live Tables
                        </Button>
                        <Button
                            variant="outline"
                            onClick={() => navigate("/actions")}
                        >
                            <Play className="mr-1.5 h-4 w-4" />
                            Run Reducers &amp; GraphQL
                        </Button>
                    </div>
                </div>
            </div>

            {/* Top Telemetry Cards */}
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                <Card className="studio-containment-panel">
                    <CardHeader className="pb-2">
                        <CardDescription className="flex items-center justify-between text-xs">
                            <span>Triedent Storage Engine</span>
                            <Cpu className="h-4 w-4 text-emerald-400" />
                        </CardDescription>
                        <CardTitle className="font-mono text-2xl">
                            0.18 ms
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="text-muted-foreground text-xs">
                        Wasm-in-Database execution across{" "}
                        <strong className="text-foreground">{totalRows}</strong>{" "}
                        live indexed rows.
                    </CardContent>
                </Card>

                <Card className="studio-containment-panel">
                    <CardHeader className="pb-2">
                        <CardDescription className="flex items-center justify-between text-xs">
                            <span>{term("lease")} Split</span>
                            <Zap className="h-4 w-4 text-amber-400" />
                        </CardDescription>
                        <CardTitle className="font-mono text-2xl text-emerald-400">
                            93% / 4% / 3%
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="text-muted-foreground text-xs">
                        93% Creator/Owner, 4% {term("mutual_shield")}, 3% SaaS
                        Operator.
                    </CardContent>
                </Card>

                <Card className="studio-containment-panel">
                    <CardHeader className="pb-2">
                        <CardDescription className="flex items-center justify-between text-xs">
                            <span>{term("tangible_ratio")}</span>
                            <ShieldCheck className="h-4 w-4 text-blue-400" />
                        </CardDescription>
                        <CardTitle className="font-mono text-2xl">
                            &ge; 5,100 bps
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="text-muted-foreground text-xs">
                        Enforces 51% minimum physical backing; halts non-par debt
                        speculation.
                    </CardContent>
                </Card>

                <Card className="studio-containment-panel">
                    <CardHeader className="pb-2">
                        <CardDescription className="flex items-center justify-between text-xs">
                            <span>Branding Dual-Locale</span>
                            <Globe className="h-4 w-4 text-purple-400" />
                        </CardDescription>
                        <CardTitle className="font-mono text-xl">
                            {locale === "institutional"
                                ? "Institutional"
                                : "Fiqh al-Mu‘amalat"}
                        </CardTitle>
                    </CardHeader>
                    <CardContent className="flex items-center justify-between text-xs">
                        <span className="text-muted-foreground">
                            17 canonical keys mapped
                        </span>
                        <button
                            type="button"
                            onClick={toggleLocale}
                            className="text-emerald-400 underline hover:text-emerald-300"
                        >
                            Switch locale
                        </button>
                    </CardContent>
                </Card>
            </div>

            {/* 1-Click Vertical Reference Simulators */}
            <div className="space-y-3">
                <div className="flex items-center justify-between">
                    <div>
                        <h2 className="flex items-center gap-2 text-base font-semibold">
                            <Sparkles className="h-4 w-4 text-emerald-400" />
                            1-Click Live Protocol Workflows
                        </h2>
                        <p className="text-muted-foreground text-xs">
                            Test end-to-end state transitions directly in Studio
                            and watch the{" "}
                            <code className="text-foreground">Triedent</code>{" "}
                            tables and event stream update in real time.
                        </p>
                    </div>
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => navigate("/sdk")}
                        className="text-xs"
                    >
                        <Code2 className="mr-1.5 h-3.5 w-3.5" />
                        View SDK Code
                    </Button>
                </div>

                <div className="grid gap-4 md:grid-cols-2">
                    {quickstarts.map((item) => (
                        <Card
                            key={item.id}
                            className="border-border/80 hover:border-emerald-500/40 flex flex-col justify-between transition-colors"
                        >
                            <CardHeader className="pb-3">
                                <div className="flex items-center justify-between gap-2">
                                    <CardTitle className="text-sm font-semibold">
                                        {item.title}
                                    </CardTitle>
                                    <Badge
                                        variant="secondary"
                                        className="font-mono text-[10px]"
                                    >
                                        {item.badge}
                                    </Badge>
                                </div>
                                <CardDescription className="text-xs leading-relaxed">
                                    {item.description}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="pt-0">
                                <Button
                                    size="sm"
                                    variant="outline"
                                    onClick={() => runVerticalWorkflow(item.id)}
                                    className="w-full justify-between border-emerald-500/30 bg-emerald-500/10 text-xs text-emerald-300 hover:bg-emerald-500/20"
                                >
                                    <span className="flex items-center gap-1.5">
                                        <CheckCircle2 className="h-3.5 w-3.5" />
                                        {item.cta}
                                    </span>
                                    <ArrowRight className="h-3.5 w-3.5" />
                                </Button>
                            </CardContent>
                        </Card>
                    ))}
                </div>
            </div>

            {/* Canonical Wasm Services & Tables Directory */}
            <div className="space-y-3">
                <div className="flex items-center justify-between">
                    <div>
                        <h2 className="flex items-center gap-2 text-base font-semibold">
                            <Layers className="h-4 w-4 text-emerald-400" />
                            Canonical Wasm Services &amp; Triedent Tables
                        </h2>
                        <p className="text-muted-foreground text-xs">
                            Click any table to inspect live rows, filter records,
                            or run GraphQL subscriptions.
                        </p>
                    </div>
                </div>

                <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                    {STUDIO_TABLES.map((tbl) => {
                        const rowCount = (tables[tbl.id] || []).length;
                        return (
                            <div
                                key={tbl.id}
                                onClick={() =>
                                    navigate(`/tables?table=${tbl.id}`)
                                }
                                className="border-border bg-card/60 hover:border-emerald-500/50 group flex cursor-pointer flex-col justify-between rounded-lg border p-3.5 transition-all"
                            >
                                <div className="space-y-1.5">
                                    <div className="flex items-center justify-between">
                                        <Badge
                                            variant="outline"
                                            className="font-mono text-[10px] text-emerald-400"
                                        >
                                            &quot;{tbl.service}&quot;
                                        </Badge>
                                        <span className="text-muted-foreground font-mono text-[11px]">
                                            {rowCount} rows
                                        </span>
                                    </div>
                                    <h3 className="font-mono text-sm font-semibold group-hover:text-emerald-400">
                                        {tbl.tableName}
                                    </h3>
                                    <p className="text-xs font-medium text-amber-300/90">
                                        {term(tbl.canonicalKey)}
                                    </p>
                                    <p className="text-muted-foreground line-clamp-2 text-[11px]">
                                        {tbl.description}
                                    </p>
                                </div>
                                <div className="mt-3 flex items-center justify-between border-t border-white/5 pt-2 text-[11px]">
                                    <span className="text-muted-foreground font-mono">
                                        PK: {tbl.primaryKey}
                                    </span>
                                    <span className="flex items-center gap-1 text-emerald-400">
                                        Open Table
                                        <ArrowRight className="h-3 w-3" />
                                    </span>
                                </div>
                            </div>
                        );
                    })}
                </div>
            </div>
        </div>
    );
};
