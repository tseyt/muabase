import React, { useState } from "react";
import { toast } from "sonner";
import {
    AlertOctagon,
    CheckCircle2,
    HeartHandshake,
    Lock,
    Scale,
    ShieldCheck,
    Sparkles,
    Trophy,
    Users,
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

import { computeSha256Hex, useStudio } from "../context/studio-context";

export const InvariantsStudioPage: React.FC = () => {
    const { term, executeStudioAction, tables } = useStudio();

    // 1. 93% / 4% / 3% Split & Charity Sink state
    const [grossUsd, setGrossUsd] = useState<number>(240);
    const [rail, setRail] = useState<string>("stripe_connect");
    const [latePenaltyUsd, setLatePenaltyUsd] = useState<number>(35);
    const [directPrepCostUsd, setDirectPrepCostUsd] = useState<number>(20);

    const owner93Usd = grossUsd * 0.93;
    const shield4Usd = grossUsd * 0.04;
    const saas3Usd = grossUsd * 0.03;

    // 2. Tangible Asset Floor state
    const [tangibleBps, setTangibleBps] = useState<number>(8500);
    const isTangibleValid = tangibleBps >= 5100;

    // 3. Dual-Witness Bayyinah Gate state
    const [receiptText, setReceiptText] = useState<string>(
        "Inspection #409: Leica M11 Returned Clean - Sensor & Mount Verified",
    );
    const [witness1Signed, setWitness1Signed] = useState<boolean>(true);
    const [witness2Signed, setWitness2Signed] = useState<boolean>(true);
    const [computedDigest, setComputedDigest] = useState<string>(
        "0x4b9e2d1c8a7f6054321fedcba9876543210fedcba9876543210fedcba9876543",
    );

    const rankings = tables.CondorcetRankingTable || [];

    const handleCommitSplit = async () => {
        const res = await executeStudioAction(
            "ext-vault",
            "generateSplitIntent",
            {
                leaseId: 101,
                railType: rail,
                grossAmountCents: Math.round(grossUsd * 100),
                lenderAccount: "asset-owner",
            },
        );
        toast.success("0-Key Split Intent Recorded", {
            description: res.message,
        });
    };

    const handleCommitLateSettlement = async () => {
        const res = await executeStudioAction(
            "lease-eng",
            "settleCancellation",
            {
                leaseId: 101,
                actualKitchenRestitution: {
                    value: Math.round(directPrepCostUsd * 10000),
                },
                punitivePenalty: {
                    value: Math.round(latePenaltyUsd * 10000),
                },
                charitySinkAccount: "waqf-sadaqah",
            },
        );
        toast.success("Zero-Riba Deterrence Settled", {
            description: res.message,
        });
    };

    const handleUpdateDigest = async (val: string) => {
        setReceiptText(val);
        const hex = await computeSha256Hex(val);
        setComputedDigest(hex);
    };

    const handleCommitAttestation = async () => {
        const quorum = (witness1Signed ? 1 : 0) + (witness2Signed ? 1 : 0);
        const res = await executeStudioAction(
            "staged-tx",
            "recordAttestation",
            {
                service: "lease-eng",
                entityId: 101,
                evidenceHash: computedDigest,
                witnessQuorum: quorum,
                metadata: { ratingBps: 9800 },
            },
        );
        if (res.ok) {
            toast.success("Dual-Witness Evidence Gate Passed", {
                description: res.message,
            });
        } else {
            toast.error("Blocked by Dual-Witness Gate", {
                description: res.message,
            });
        }
    };

    return (
        <div className="mx-auto max-w-7xl space-y-6">
            <div>
                <h1 className="flex items-center gap-2 text-2xl font-bold tracking-tight">
                    <ShieldCheck className="h-6 w-6 text-emerald-400" />
                    Commercial Invariants &amp; Protocol Flows Studio
                </h1>
                <p className="text-muted-foreground text-xs">
                    Interactive visualizers for Muabase&apos;s hardcoded state
                    transition invariants: non-extractive 93%/4%/3% splits,
                    51% tangible floors, dual-witness gates, and Condorcet
                    ranking.
                </p>
            </div>

            <div className="grid gap-6 lg:grid-cols-2">
                {/* 1. 93% / 4% / 3% Split + Charity Sink Simulator */}
                <Card className="studio-containment-panel">
                    <CardHeader>
                        <div className="flex items-center justify-between">
                            <CardTitle className="flex items-center gap-2 text-base">
                                <Zap className="h-4 w-4 text-emerald-400" />
                                1. 93% / 4% / 3% Split &amp; {term("charity_sink")}
                            </CardTitle>
                            <Badge
                                variant="outline"
                                className="font-mono text-[10px] text-emerald-400"
                            >
                                lease-eng + ext-vault
                            </Badge>
                        </div>
                        <CardDescription className="text-xs">
                            Replaces 25%–35% Web2 marketplace take-rates with a
                            3% SaaS fee, 4% {term("mutual_shield")}, and 93%
                            creator/owner payout.
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-4">
                        <div className="space-y-2">
                            <div className="flex items-center justify-between text-xs">
                                <span className="font-medium">
                                    Gross Checkout / Lease Amount:
                                </span>
                                <span className="font-mono text-base font-bold text-emerald-400">
                                    ${grossUsd.toFixed(2)} (
                                    {Math.round(grossUsd * 10000)} units)
                                </span>
                            </div>
                            <input
                                type="range"
                                min={20}
                                max={2000}
                                step={10}
                                value={grossUsd}
                                onChange={(e) =>
                                    setGrossUsd(Number(e.target.value))
                                }
                                aria-label="Gross checkout amount in USD"
                                className="w-full accent-emerald-500"
                            />
                        </div>

                        {/* Visual Bar */}
                        <div className="space-y-1.5">
                            <div className="flex h-4 w-full overflow-hidden rounded-full">
                                <div
                                    style={{ width: "93%" }}
                                    className="bg-emerald-500"
                                    title="93% Asset Owner / Tastemaker"
                                />
                                <div
                                    style={{ width: "4%" }}
                                    className="bg-blue-500"
                                    title="4% Mutual Shield Reserve"
                                />
                                <div
                                    style={{ width: "3%" }}
                                    className="bg-amber-500"
                                    title="3% SaaS Node Operator"
                                />
                            </div>
                            <div className="grid grid-cols-3 gap-2 pt-1 font-mono text-xs">
                                <div className="rounded border border-emerald-500/30 bg-emerald-500/10 p-2">
                                    <div className="text-[10px] text-emerald-300">
                                        93% Owner (9300 bps)
                                    </div>
                                    <div className="font-bold">
                                        ${owner93Usd.toFixed(2)}
                                    </div>
                                </div>
                                <div className="rounded border border-blue-500/30 bg-blue-500/10 p-2">
                                    <div className="text-[10px] text-blue-300">
                                        4% Reserve (400 bps)
                                    </div>
                                    <div className="font-bold">
                                        ${shield4Usd.toFixed(2)}
                                    </div>
                                </div>
                                <div className="rounded border border-amber-500/30 bg-amber-500/10 p-2">
                                    <div className="text-[10px] text-amber-300">
                                        3% SaaS (300 bps)
                                    </div>
                                    <div className="font-bold">
                                        ${saas3Usd.toFixed(2)}
                                    </div>
                                </div>
                            </div>
                        </div>

                        {/* Settlement Rail Selector */}
                        <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
                            <div className="flex items-center gap-1.5 text-xs">
                                <span className="text-muted-foreground">
                                    0-Key Rail:
                                </span>
                                {[
                                    "stripe_connect",
                                    "bitcoin_taproot",
                                    "evm_safe",
                                ].map((r) => (
                                    <button
                                        key={r}
                                        type="button"
                                        onClick={() => setRail(r)}
                                        className={`rounded px-2 py-1 font-mono text-[11px] ${
                                            rail === r
                                                ? "bg-emerald-600 text-white"
                                                : "bg-muted text-muted-foreground"
                                        }`}
                                    >
                                        {r}
                                    </button>
                                ))}
                            </div>
                            <Button
                                size="sm"
                                onClick={handleCommitSplit}
                                className="bg-emerald-600 text-xs text-white hover:bg-emerald-500"
                            >
                                Commit Split Intent
                            </Button>
                        </div>

                        {/* Zero-Penalty Profiteering Charity Sink Box */}
                        <div className="border-border bg-muted/30 space-y-2.5 rounded-lg border p-3">
                            <div className="flex items-center justify-between">
                                <span className="flex items-center gap-1.5 text-xs font-semibold text-amber-300">
                                    <HeartHandshake className="h-4 w-4" />
                                    Zero-Penalty Profiteering Guard (
                                    <code>waqf-sadaqah</code>)
                                </span>
                                <Badge variant="outline" className="text-[10px]">
                                    100% Charity Surcharge Routing
                                </Badge>
                            </div>
                            <div className="grid grid-cols-2 gap-3 text-xs">
                                <div>
                                    <label className="text-muted-foreground block text-[11px]">
                                        Direct Host Prep Cost ($):
                                    </label>
                                    <input
                                        type="number"
                                        min={0}
                                        value={directPrepCostUsd}
                                        onChange={(e) =>
                                            setDirectPrepCostUsd(
                                                Number(e.target.value),
                                            )
                                        }
                                        className="border-border bg-background mt-1 w-full rounded border px-2 py-1 font-mono text-xs"
                                    />
                                </div>
                                <div>
                                    <label className="text-muted-foreground block text-[11px]">
                                        Punitive Late Deterrence ($):
                                    </label>
                                    <input
                                        type="number"
                                        min={0}
                                        value={latePenaltyUsd}
                                        onChange={(e) =>
                                            setLatePenaltyUsd(
                                                Number(e.target.value),
                                            )
                                        }
                                        className="border-border bg-background mt-1 w-full rounded border px-2 py-1 font-mono text-xs"
                                    />
                                </div>
                            </div>
                            <div className="flex items-center justify-between pt-1 text-xs">
                                <span className="text-muted-foreground">
                                    Host receives{" "}
                                    <strong className="text-foreground">
                                        ${directPrepCostUsd}
                                    </strong>{" "}
                                    restitution;{" "}
                                    <strong className="text-amber-300">
                                        ${latePenaltyUsd}
                                    </strong>{" "}
                                    routes to <code>waqf-sadaqah</code>.
                                </span>
                                <Button
                                    size="sm"
                                    variant="outline"
                                    onClick={handleCommitLateSettlement}
                                    className="h-7 text-xs"
                                >
                                    Settle
                                </Button>
                            </div>
                        </div>
                    </CardContent>
                </Card>

                {/* 2. Tangible Asset Backing Floor (>= 5100 bps) */}
                <Card className="studio-containment-panel">
                    <CardHeader>
                        <div className="flex items-center justify-between">
                            <CardTitle className="flex items-center gap-2 text-base">
                                <Scale className="h-4 w-4 text-blue-400" />
                                2. {term("tangible_ratio")} &amp;{" "}
                                {term("priority_window")}
                            </CardTitle>
                            <Badge
                                variant="outline"
                                className="font-mono text-[10px] text-blue-400"
                            >
                                tokens + token-swap
                            </Badge>
                        </div>
                        <CardDescription className="text-xs">
                            Secondary trading of RWA shares at non-par is
                            mathematically halted if tangible physical asset
                            backing drops below 51% (5,100 bps).
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-4">
                        <div className="space-y-2">
                            <div className="flex items-center justify-between text-xs">
                                <span className="font-medium">
                                    Tangible Physical Asset Backing Ratio:
                                </span>
                                <span
                                    className={`font-mono text-base font-bold ${
                                        isTangibleValid
                                            ? "text-emerald-400"
                                            : "text-rose-400"
                                    }`}
                                >
                                    {tangibleBps} bps (
                                    {(tangibleBps / 100).toFixed(2)}%)
                                </span>
                            </div>
                            <input
                                type="range"
                                min={1000}
                                max={10000}
                                step={100}
                                value={tangibleBps}
                                onChange={(e) =>
                                    setTangibleBps(Number(e.target.value))
                                }
                                aria-label="Tangible asset backing ratio in basis points"
                                className="w-full accent-blue-500"
                            />
                            <div className="text-muted-foreground flex justify-between font-mono text-[10px]">
                                <span>1000 bps (10% Cash/Debt)</span>
                                <span className="text-amber-300 font-semibold">
                                    | 5100 bps (51% Floor)
                                </span>
                                <span>10000 bps (100% Tangible)</span>
                            </div>
                        </div>

                        <div
                            className={`flex items-start gap-3 rounded-lg border p-3.5 text-xs ${
                                isTangibleValid
                                    ? "border-emerald-500/40 bg-emerald-500/10"
                                    : "border-rose-500/40 bg-rose-500/10"
                            }`}
                        >
                            {isTangibleValid ? (
                                <CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-emerald-400" />
                            ) : (
                                <AlertOctagon className="mt-0.5 h-5 w-5 shrink-0 text-rose-400" />
                            )}
                            <div className="space-y-1">
                                <div className="font-semibold">
                                    {isTangibleValid
                                        ? "Secondary Market Active — Tangible Backing Verified (>= 5100 bps)"
                                        : "HALTED: Bay' al-Dayn Violation (tangibleRatioBps < 5100)"}
                                </div>
                                <p className="text-muted-foreground">
                                    {isTangibleValid
                                        ? "Physical real estate, solar hardware, or luxury inventory exceeds 51% of pool value. Co-owners enjoy a 24h Right of First Refusal (86,400s) before outside offers."
                                        : "Non-par secondary trading is rejected at the Wasm state transition boundary to prevent naked debt speculation and predatory debt discounting."}
                                </p>
                            </div>
                        </div>

                        <div className="flex items-center justify-between pt-2">
                            <span className="text-muted-foreground font-mono text-xs">
                                priorityWindowSec: 86400 (24h Co-Owner Pre-emption)
                            </span>
                            <Button
                                size="sm"
                                variant={
                                    isTangibleValid ? "default" : "destructive"
                                }
                                onClick={async () => {
                                    const res = await executeStudioAction(
                                        "tokens",
                                        "configureAssetPool",
                                        {
                                            config: {
                                                tokenId: Math.floor(
                                                    1100 + Math.random() * 900,
                                                ),
                                                title: `Simulated Series (${(tangibleBps / 100).toFixed(0)}% Tangible)`,
                                                tangibleRatioBps: tangibleBps,
                                                minTrustScore: 7000,
                                                priorityWindowSec: 86400,
                                                charitySinkAccount:
                                                    "waqf-sadaqah",
                                            },
                                        },
                                    );
                                    if (res.ok) {
                                        toast.success(res.message);
                                    } else {
                                        toast.error(res.message);
                                    }
                                }}
                                className="text-xs"
                            >
                                {isTangibleValid
                                    ? "Register Pool in Triedent"
                                    : "Test Revert in Triedent"}
                            </Button>
                        </div>
                    </CardContent>
                </Card>

                {/* 3. Dual-Witness Evidentiary Gate (Binary Bayyinah) */}
                <Card className="studio-containment-panel">
                    <CardHeader>
                        <div className="flex items-center justify-between">
                            <CardTitle className="flex items-center gap-2 text-base">
                                <Lock className="h-4 w-4 text-purple-400" />
                                3. {term("evidence_gate")} (SHA-256 + Quorum &ge; 2)
                            </CardTitle>
                            <Badge
                                variant="outline"
                                className="font-mono text-[10px] text-purple-400"
                            >
                                staged-tx + credentials
                            </Badge>
                        </div>
                        <CardDescription className="text-xs">
                            Eliminates bot astroturfing and fake claims by
                            requiring a 32-byte SHA-256 Ricardian digest co-signed
                            by at least 2 soulbound witnesses.
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-3.5">
                        <div className="space-y-1">
                            <label className="text-xs font-medium">
                                Physical Proof / POS Check / Inspection Payload:
                            </label>
                            <input
                                type="text"
                                value={receiptText}
                                onChange={(e) =>
                                    handleUpdateDigest(e.target.value)
                                }
                                className="border-border bg-background w-full rounded border px-2.5 py-1.5 font-mono text-xs"
                            />
                        </div>

                        <div className="bg-muted/40 rounded border border-white/10 p-2.5 font-mono text-[11px]">
                            <div className="text-muted-foreground">
                                H(M) = crypto.subtle.digest(&apos;SHA-256&apos;,
                                payload):
                            </div>
                            <div className="truncate text-amber-300">
                                {computedDigest}
                            </div>
                        </div>

                        <div className="grid grid-cols-2 gap-3">
                            <label className="border-border bg-card/60 flex cursor-pointer items-center gap-2 rounded-lg border p-2.5 text-xs">
                                <input
                                    type="checkbox"
                                    checked={witness1Signed}
                                    onChange={(e) =>
                                        setWitness1Signed(e.target.checked)
                                    }
                                    className="rounded accent-emerald-500"
                                />
                                <div>
                                    <div className="font-mono font-semibold">
                                        Witness #1: @alice
                                    </div>
                                    <div className="text-muted-foreground text-[10px]">
                                        trust_score: 9450 bps
                                    </div>
                                </div>
                            </label>

                            <label className="border-border bg-card/60 flex cursor-pointer items-center gap-2 rounded-lg border p-2.5 text-xs">
                                <input
                                    type="checkbox"
                                    checked={witness2Signed}
                                    onChange={(e) =>
                                        setWitness2Signed(e.target.checked)
                                    }
                                    className="rounded accent-emerald-500"
                                />
                                <div>
                                    <div className="font-mono font-semibold">
                                        Witness #2: @inspector-1
                                    </div>
                                    <div className="text-muted-foreground text-[10px]">
                                        trust_score: 9910 bps
                                    </div>
                                </div>
                            </label>
                        </div>

                        <div className="flex items-center justify-between pt-1">
                            <span className="font-mono text-xs">
                                Active Witness Quorum:{" "}
                                <strong
                                    className={
                                        witness1Signed && witness2Signed
                                            ? "text-emerald-400"
                                            : "text-rose-400"
                                    }
                                >
                                    {(witness1Signed ? 1 : 0) +
                                        (witness2Signed ? 1 : 0)}{" "}
                                    / 2 required
                                </strong>
                            </span>
                            <Button
                                size="sm"
                                onClick={handleCommitAttestation}
                                className="bg-emerald-600 text-xs text-white hover:bg-emerald-500"
                            >
                                Record Attestation in StagedTx
                            </Button>
                        </div>
                    </CardContent>
                </Card>

                {/* 4. Wasm Schulze Condorcet Pairwise Tournament */}
                <Card className="studio-containment-panel">
                    <CardHeader>
                        <div className="flex items-center justify-between">
                            <CardTitle className="flex items-center gap-2 text-base">
                                <Trophy className="h-4 w-4 text-amber-400" />
                                4. {term("evals")} (Schulze Condorcet Matrix)
                            </CardTitle>
                            <Badge
                                variant="outline"
                                className="font-mono text-[10px] text-amber-400"
                            >
                                evals (Zero People-Ranking)
                            </Badge>
                        </div>
                        <CardDescription className="text-xs">
                            Humans are never ranked. Verified deliverables,
                            restaurants, and claims are ranked via un-gameable
                            pairwise Condorcet comparisons in Wasm.
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-3">
                        <div className="space-y-2">
                            {rankings.map((item) => (
                                <div
                                    key={item.entityId}
                                    className="border-border bg-muted/25 flex items-center justify-between rounded-lg border px-3 py-2 text-xs"
                                >
                                    <div className="flex items-center gap-2.5">
                                        <span className="flex h-6 w-6 items-center justify-center rounded-full bg-emerald-500/20 font-mono text-xs font-bold text-emerald-400">
                                            #{item.schulzeRank}
                                        </span>
                                        <div>
                                            <div className="font-medium">
                                                {item.entityName}
                                            </div>
                                            <div className="text-muted-foreground font-mono text-[10px]">
                                                Entity #{item.entityId} &bull;{" "}
                                                {item.category}
                                            </div>
                                        </div>
                                    </div>
                                    <div className="text-right font-mono">
                                        <div className="text-emerald-400">
                                            {item.pairwiseWins}W –{" "}
                                            {item.pairwiseLosses}L
                                        </div>
                                        <div className="text-muted-foreground text-[10px]">
                                            Net: +
                                            {Number(item.pairwiseWins) -
                                                Number(item.pairwiseLosses)}
                                        </div>
                                    </div>
                                </div>
                            ))}
                        </div>

                        <div className="flex flex-wrap items-center justify-between gap-2 pt-2">
                            <Button
                                size="sm"
                                variant="outline"
                                onClick={async () => {
                                    const res = await executeStudioAction(
                                        "evals",
                                        "recordPairwiseVote",
                                        {
                                            winnerId: 3001,
                                            loserId: 2042,
                                            category: "food",
                                        },
                                    );
                                    toast.success(res.message);
                                }}
                                className="text-xs"
                            >
                                <Sparkles className="mr-1.5 h-3.5 w-3.5 text-emerald-400" />
                                Vote #3001 &gt; #2042
                            </Button>

                            <Button
                                size="sm"
                                variant="outline"
                                onClick={async () => {
                                    const res = await executeStudioAction(
                                        "evals",
                                        "recordPairwiseVote",
                                        {
                                            winnerId: 2042,
                                            loserId: 3001,
                                            category: "food",
                                        },
                                    );
                                    toast.success(res.message);
                                }}
                                className="text-xs"
                            >
                                <Users className="mr-1.5 h-3.5 w-3.5 text-amber-400" />
                                Vote #2042 &gt; #3001
                            </Button>
                        </div>
                    </CardContent>
                </Card>
            </div>
        </div>
    );
};
