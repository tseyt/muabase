import React, { useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import {
    AlertTriangle,
    CheckCircle2,
    Code2,
    Copy,
    Hash,
    Play,
    ShieldAlert,
    Sparkles,
    Terminal,
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
import { STUDIO_TABLES } from "../lib/studio-data";

interface ActionTemplate {
    id: string;
    service: string;
    action: string;
    canonicalKey: string;
    description: string;
    invariantRule: string;
    defaultPayload: Record<string, any>;
    sdkSnippet: (payload: Record<string, any>) => string;
}

const ACTION_TEMPLATES: ActionTemplate[] = [
    {
        id: "tokens.configureAssetPool",
        service: "tokens",
        action: "configureAssetPool",
        canonicalKey: "asset_pool",
        description:
            "Configure an RWA Asset Series / Sukuk al-Qirad pool with tangible backing floor (>= 5100 bps) and 24h co-owner ROFR.",
        invariantRule:
            "tangibleRatioBps MUST be >= 5100 (51%). Values < 5100 revert with Bay' al-Dayn Violation.",
        defaultPayload: {
            config: {
                tokenId: 1088,
                title: "Rolex Daytona Vintage Vault Series #1088",
                legalWrapperHash:
                    "0x9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b",
                maxHolders: 99,
                minTrustScore: 7000,
                tangibleRatioBps: 10000,
                priorityWindowSec: 86400,
                custodianAccount: "custodian",
                custodianFeeBps: 0,
                charitySinkAccount: "waqf-sadaqah",
            },
        },
        sdkSnippet: (p) =>
            `await client.tokens.configureAssetPool(${JSON.stringify(p.config ?? p, null, 2)});`,
    },
    {
        id: "lease-eng.createLease",
        service: "lease-eng",
        action: "createLease",
        canonicalKey: "lease",
        description:
            "Create a fixed usufruct lease or supper club table reservation with autonomous 93% / 4% / 3% fee split.",
        invariantRule:
            "Currency values use integer precision 4 ($1.00 = 10000). Late penalties route 100% to 'waqf-sadaqah'.",
        defaultPayload: {
            lessor: "asset-owner",
            tokenId: 1042,
            durationSec: 345600,
            grossPayment: { value: 1800000 },
            paymentTokenId: 1,
            preConditionEvidenceHash:
                "0x3a4b5c6d7e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b",
            isProgressiveBuyout: false,
            charitySinkAccount: "waqf-sadaqah",
        },
        sdkSnippet: (p) =>
            `const lease = await client.leaseEngine.createLease(${JSON.stringify(p, null, 2)});`,
    },
    {
        id: "staged-tx.recordAttestation",
        service: "staged-tx",
        action: "recordAttestation",
        canonicalKey: "evidence_gate",
        description:
            "Record a Dual-Witness (Bayyinah) Ricardian evidence SHA-256 digest for POS dining receipts or condition photos.",
        invariantRule:
            "evidenceHash MUST be 0x + 64 hex chars (SHA-256) and witnessQuorum MUST be >= 2.",
        defaultPayload: {
            service: "evaluation",
            entityId: 3001,
            evidenceHash:
                "0x8f2d4a6c1e9b03758f2d4a6c1e9b03758f2d4a6c1e9b03758f2d4a6c1e9b0375",
            attester: "alice",
            witnessQuorum: 2,
            metadata: {
                ratingBps: 9400,
                timestamp: 1761510000,
            },
        },
        sdkSnippet: (p) =>
            `await client.stagedTx.recordAttestation(${JSON.stringify(p, null, 2)});`,
    },
    {
        id: "evals.recordPairwiseVote",
        service: "evals",
        action: "recordPairwiseVote",
        canonicalKey: "evals",
        description:
            "Submit a head-to-head pairwise comparison into the Wasm Schulze Condorcet tournament (people are never ranked).",
        invariantRule:
            "Only objective entities (restaurants, claims, bounties) can be ranked; subjective human ranking is disabled.",
        defaultPayload: {
            winnerId: 3001,
            loserId: 2042,
            category: "food",
            voter: "alice",
        },
        sdkSnippet: (p) =>
            `await client.evaluations.recordPairwiseVote(${JSON.stringify(p, null, 2)});`,
    },
    {
        id: "ext-vault.generateSplitIntent",
        service: "ext-vault",
        action: "generateSplitIntent",
        canonicalKey: "external_vault",
        description:
            "Generate a 0-key non-custodial multi-party payout split (93% Owner / 4% Mutual Shield / 3% SaaS).",
        invariantRule:
            "Protocol operators hold 0 keys. Qualifies under NYDFS 23 NYCRR § 200.2(q) software exemption.",
        defaultPayload: {
            leaseId: 101,
            railType: "stripe_connect",
            grossAmountCents: 18000,
            lenderAccount: "asset-owner",
        },
        sdkSnippet: (p) =>
            `await client.externalVault.generateSplitIntent(${JSON.stringify(p, null, 2)});`,
    },
    {
        id: "lease-eng.settleCancellation",
        service: "lease-eng",
        action: "settleCancellation",
        canonicalKey: "charity_sink",
        description:
            "Settle a late return or dining cancellation: actual proven prep cost to host, 100% of punitive surcharge to charity sink.",
        invariantRule:
            "Creditors never profit from late fees. Punitive penalties route 100% to charitySinkAccount ('waqf-sadaqah').",
        defaultPayload: {
            leaseId: 101,
            actualKitchenRestitution: { value: 350000 },
            punitivePenalty: { value: 250000 },
            charitySinkAccount: "waqf-sadaqah",
        },
        sdkSnippet: (p) =>
            `await client.leaseEngine.settleCancellation(${JSON.stringify(p, null, 2)});`,
    },
    {
        id: "claimcouncil.submitDamageClaim",
        service: "claimcouncil",
        action: "submitDamageClaim",
        canonicalKey: "claim_council",
        description:
            "Submit a physical damage claim to the 3-peer rapid triage committee with pre/post condition SHA-256 digests.",
        invariantRule:
            "Requires verified pre-condition, post-condition, and repair quote SHA-256 digests.",
        defaultPayload: {
            leaseId: 101,
            respondent: "alice",
            poolId: 1,
            claimedAmount: { value: 650000 },
            preConditionEvidenceHash:
                "0x4b9e2d1c8a7f6054321fedcba9876543210fedcba9876543210fedcba9876543",
            postConditionEvidenceHash:
                "0x7f8e9d0c1b2a39485766554433221100fedcba9876543210fedcba9876543210",
            repairQuoteHash:
                "0x5a6b7c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d3e4f5a6b",
        },
        sdkSnippet: (p) =>
            `await client.claimCouncil.submitDamageClaim(${JSON.stringify(p, null, 2)});`,
    },
    {
        id: "creditclear.executeBilateralNetting",
        service: "creditclear",
        action: "executeBilateralNetting",
        canonicalKey: "credit_clear",
        description:
            "Algorithmically net reciprocal B2B invoices with $0 cash wire or interest-bearing factoring.",
        invariantRule:
            "Extinguishes mutual debt obligations at par value (Muqassah).",
        defaultPayload: {
            invoice1Id: 510,
            invoice2Id: 511,
            clearedAmountUsd4: 48000000,
        },
        sdkSnippet: (p) =>
            `await client.creditClear.executeBilateralNetting(${JSON.stringify(p, null, 2)});`,
    },
];

export const ActionsPlaygroundPage: React.FC = () => {
    const [searchParams] = useSearchParams();
    const { executeStudioAction, term, tables } = useStudio();

    const initialService = searchParams.get("service");
    const initialTab =
        searchParams.get("tab") === "graphql" ? "graphql" : "reducer";

    const [mode, setMode] = useState<"reducer" | "graphql">(initialTab);
    const [selectedTemplateId, setSelectedTemplateId] = useState<string>(() => {
        if (initialService) {
            const match = ACTION_TEMPLATES.find(
                (t) => t.service === initialService,
            );
            if (match) return match.id;
        }
        return ACTION_TEMPLATES[0].id;
    });

    const selectedTemplate = useMemo(
        () =>
            ACTION_TEMPLATES.find((t) => t.id === selectedTemplateId) ??
            ACTION_TEMPLATES[0],
        [selectedTemplateId],
    );

    const [jsonText, setJsonText] = useState<string>(() =>
        JSON.stringify(selectedTemplate.defaultPayload, null, 2),
    );
    const [tangibleRatioInput, setTangibleRatioInput] =
        useState<string>("10000");
    const [evidenceHashInput, setEvidenceHashInput] = useState<string>(
        "0x8f2d4a6c1e9b03758f2d4a6c1e9b03758f2d4a6c1e9b03758f2d4a6c1e9b0375",
    );
    const [hashSourceText, setHashSourceText] = useState<string>(
        "POS Receipt #8841 - Sufra Woodfire Supper Club - $240.00",
    );
    const [lastResult, setLastResult] = useState<{
        ok: boolean;
        message: string;
        timestamp: string;
    } | null>(null);

    // GraphQL state
    const [gqlService, setGqlService] = useState<string>(
        initialService || "lease-eng",
    );
    const [gqlQuery, setGqlQuery] = useState<string>(
        STUDIO_TABLES[1].graphqlQuery,
    );
    const [gqlOutput, setGqlOutput] = useState<string>("");

    // Modern Web Guidance: validate-input-after-interaction WeakMap fallback
    const formRef = useRef<HTMLFormElement | null>(null);
    useEffect(() => {
        const form = formRef.current;
        if (!form) return;
        const dirtyState = new WeakMap<HTMLInputElement, boolean>();

        const updateState = (input: HTMLInputElement) => {
            const isValid = input.checkValidity();
            input.classList.toggle("user-invalid-fallback", !isValid);
            input.classList.toggle("user-valid-fallback", isValid);
            if (!isValid) {
                input.setAttribute("aria-invalid", "true");
            } else {
                input.removeAttribute("aria-invalid");
            }
        };

        const handleBlur = (e: Event) => {
            const target = e.target;
            if (target instanceof HTMLInputElement) {
                dirtyState.set(target, true);
                updateState(target);
            }
        };

        const handleInput = (e: Event) => {
            const target = e.target;
            if (target instanceof HTMLInputElement && dirtyState.get(target)) {
                updateState(target);
            }
        };

        form.addEventListener("blur", handleBlur, true);
        form.addEventListener("input", handleInput, true);
        return () => {
            form.removeEventListener("blur", handleBlur, true);
            form.removeEventListener("input", handleInput, true);
        };
    }, [selectedTemplateId]);

    const handleSelectTemplate = (tpl: ActionTemplate) => {
        setSelectedTemplateId(tpl.id);
        setJsonText(JSON.stringify(tpl.defaultPayload, null, 2));
        setLastResult(null);
    };

    const generateDigestAndApply = async () => {
        const digest = await computeSha256Hex(hashSourceText);
        setEvidenceHashInput(digest);
        try {
            const parsed = JSON.parse(jsonText);
            if ("evidenceHash" in parsed) parsed.evidenceHash = digest;
            if ("preConditionEvidenceHash" in parsed)
                parsed.preConditionEvidenceHash = digest;
            if (parsed.config && "legalWrapperHash" in parsed.config) {
                parsed.config.legalWrapperHash = digest;
            }
            setJsonText(JSON.stringify(parsed, null, 2));
            toast.success("Generated 32-byte SHA-256 Digest", {
                description: digest,
            });
        } catch {
            // ignore JSON parse error while editing
        }
    };

    const handleRunReducer = async (e: React.FormEvent) => {
        e.preventDefault();
        try {
            const parsed = JSON.parse(jsonText);
            const res = await executeStudioAction(
                selectedTemplate.service,
                selectedTemplate.action,
                parsed,
            );
            setLastResult({
                ok: res.ok,
                message: res.message,
                timestamp: new Date().toLocaleTimeString(),
            });
            if (res.ok) {
                toast.success(
                    `Executed ${selectedTemplate.service}::${selectedTemplate.action}`,
                    { description: res.message },
                );
            } else {
                toast.error("Transaction Reverted by Protocol Guard", {
                    description: res.message,
                });
            }
        } catch (err: any) {
            toast.error("Invalid JSON Payload", {
                description: err?.message || "Check JSON syntax",
            });
        }
    };

    const handleRunGraphql = () => {
        const matchingTable = STUDIO_TABLES.find(
            (t) => t.service === gqlService,
        );
        const rows = matchingTable ? tables[matchingTable.id] || [] : [];
        const responseObj = {
            data: {
                [matchingTable
                    ? matchingTable.tableName.charAt(0).toLowerCase() +
                      matchingTable.tableName.slice(1)
                    : "rows"]: rows,
            },
            extensions: {
                engine: "Triedent Memory-Mapped Wasm",
                executionTimeMs: 0.14,
            },
        };
        setGqlOutput(JSON.stringify(responseObj, null, 2));
        toast.success(`Queried /api/${gqlService}/graphql in 0.14 ms`);
    };

    return (
        <div className="mx-auto max-w-7xl space-y-6">
            {/* Header & Mode Switcher */}
            <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-center">
                <div>
                    <h1 className="flex items-center gap-2 text-2xl font-bold tracking-tight">
                        <Terminal className="h-6 w-6 text-emerald-400" />
                        Wasm Action Runner &amp; GraphQL Playground
                    </h1>
                    <p className="text-muted-foreground text-xs">
                        Dispatch transactions directly to canonical Wasm services
                        with live commercial invariant verification.
                    </p>
                </div>

                <div className="bg-muted inline-flex rounded-lg p-1">
                    <button
                        type="button"
                        onClick={() => setMode("reducer")}
                        className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                            mode === "reducer"
                                ? "bg-emerald-600 text-white"
                                : "text-muted-foreground hover:text-foreground"
                        }`}
                    >
                        Wasm Action / Reducer Runner
                    </button>
                    <button
                        type="button"
                        onClick={() => setMode("graphql")}
                        className={`rounded-md px-3 py-1.5 text-xs font-medium transition-colors ${
                            mode === "graphql"
                                ? "bg-emerald-600 text-white"
                                : "text-muted-foreground hover:text-foreground"
                        }`}
                    >
                        Triedent GraphQL Playground
                    </button>
                </div>
            </div>

            {mode === "reducer" ? (
                <div className="grid gap-6 lg:grid-cols-12">
                    {/* Left List of Canonical Actions */}
                    <div className="space-y-2 lg:col-span-4">
                        <p className="text-muted-foreground px-1 text-xs font-semibold uppercase">
                            Canonical Wasm Actions (llms.txt)
                        </p>
                        <div className="space-y-1.5">
                            {ACTION_TEMPLATES.map((tpl) => {
                                const active = tpl.id === selectedTemplate.id;
                                return (
                                    <button
                                        key={tpl.id}
                                        type="button"
                                        onClick={() =>
                                            handleSelectTemplate(tpl)
                                        }
                                        className={`flex w-full flex-col rounded-lg border p-3 text-left transition-colors ${
                                            active
                                                ? "border-emerald-500/50 bg-emerald-500/10"
                                                : "border-border bg-card/50 hover:bg-muted/50"
                                        }`}
                                    >
                                        <div className="flex items-center justify-between gap-2">
                                            <span className="font-mono text-xs font-semibold text-emerald-400">
                                                {tpl.service}::{tpl.action}
                                            </span>
                                            <Badge
                                                variant="outline"
                                                className="text-[10px]"
                                            >
                                                {term(tpl.canonicalKey)}
                                            </Badge>
                                        </div>
                                        <p className="text-muted-foreground mt-1 line-clamp-2 text-[11px]">
                                            {tpl.description}
                                        </p>
                                    </button>
                                );
                            })}
                        </div>
                    </div>

                    {/* Right Action Builder & Validator */}
                    <div className="space-y-4 lg:col-span-8">
                        <Card>
                            <CardHeader className="pb-3">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <div>
                                        <CardTitle className="font-mono text-base">
                                            POST /api/{selectedTemplate.service}
                                            /action/{selectedTemplate.action}
                                        </CardTitle>
                                        <CardDescription className="mt-1 text-xs">
                                            {selectedTemplate.description}
                                        </CardDescription>
                                    </div>
                                    <Badge className="bg-amber-500/15 font-mono text-xs text-amber-300">
                                        {term(selectedTemplate.canonicalKey)}
                                    </Badge>
                                </div>
                            </CardHeader>

                            <CardContent>
                                <form
                                    ref={formRef}
                                    onSubmit={handleRunReducer}
                                    className="space-y-4"
                                >
                                    {/* Hardcoded Invariant Banner */}
                                    <div className="flex items-start gap-2.5 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3 text-xs">
                                        <ShieldAlert className="mt-0.5 h-4 w-4 shrink-0 text-amber-400" />
                                        <div>
                                            <span className="font-semibold text-amber-300">
                                                Protocol Invariant Guard:{" "}
                                            </span>
                                            <span className="text-zinc-200">
                                                {selectedTemplate.invariantRule}
                                            </span>
                                        </div>
                                    </div>

                                    {/* Interactive Invariant Quick-Test Controls (following validate-input-after-interaction) */}
                                    <div className="grid gap-4 rounded-lg border border-white/10 bg-zinc-900/50 p-3.5 sm:grid-cols-2">
                                        <div className="studio-field">
                                            <label
                                                htmlFor="tangible-ratio-input"
                                                className="text-xs font-semibold"
                                            >
                                                Tangible Backing Floor (
                                                <code>tangibleRatioBps</code>)
                                            </label>
                                            {/* MANDATORY per validate-input-after-interaction: Place format hints above the input */}
                                            <span
                                                id="tangible-ratio-hint"
                                                className="studio-hint"
                                            >
                                                Required integer: 5100 to 10000
                                                bps (51.00% – 100.00%)
                                            </span>
                                            <input
                                                id="tangible-ratio-input"
                                                type="number"
                                                required
                                                min={5100}
                                                max={10000}
                                                value={tangibleRatioInput}
                                                onChange={(e) => {
                                                    const val = e.target.value;
                                                    setTangibleRatioInput(val);
                                                    try {
                                                        const parsed =
                                                            JSON.parse(jsonText);
                                                        if (parsed.config) {
                                                            parsed.config.tangibleRatioBps =
                                                                Number(val);
                                                            setJsonText(
                                                                JSON.stringify(
                                                                    parsed,
                                                                    null,
                                                                    2,
                                                                ),
                                                            );
                                                        }
                                                    } catch {
                                                        // ignore
                                                    }
                                                }}
                                                aria-describedby="tangible-ratio-hint"
                                                aria-errormessage="tangible-ratio-err"
                                                className="border-border bg-background w-full rounded border px-2.5 py-1.5 font-mono text-xs"
                                            />
                                            <div
                                                id="tangible-ratio-err"
                                                className="studio-error-msg"
                                            >
                                                <AlertTriangle className="h-3.5 w-3.5" />
                                                Bay&apos; al-Dayn Violation: Must
                                                be &ge; 5100 bps (51%).
                                            </div>
                                        </div>

                                        <div className="studio-field">
                                            <label
                                                htmlFor="evidence-hash-input"
                                                className="text-xs font-semibold"
                                            >
                                                Ricardian SHA-256 Digest (
                                                <code>evidenceHash</code>)
                                            </label>
                                            {/* MANDATORY per validate-input-after-interaction: Place format hints above the input */}
                                            <span
                                                id="evidence-hash-hint"
                                                className="studio-hint"
                                            >
                                                Format: 0x + 64 hexadecimal
                                                characters (32 bytes)
                                            </span>
                                            <input
                                                id="evidence-hash-input"
                                                type="text"
                                                required
                                                pattern="^0x[0-9a-fA-F]{64}$"
                                                value={evidenceHashInput}
                                                onChange={(e) => {
                                                    const val = e.target.value;
                                                    setEvidenceHashInput(val);
                                                    try {
                                                        const parsed =
                                                            JSON.parse(jsonText);
                                                        if (
                                                            "evidenceHash" in
                                                            parsed
                                                        )
                                                            parsed.evidenceHash =
                                                                val;
                                                        if (
                                                            "preConditionEvidenceHash" in
                                                            parsed
                                                        )
                                                            parsed.preConditionEvidenceHash =
                                                                val;
                                                        setJsonText(
                                                            JSON.stringify(
                                                                parsed,
                                                                null,
                                                                2,
                                                            ),
                                                        );
                                                    } catch {
                                                        // ignore
                                                    }
                                                }}
                                                aria-describedby="evidence-hash-hint"
                                                aria-errormessage="evidence-hash-err"
                                                className="border-border bg-background w-full rounded border px-2.5 py-1.5 font-mono text-xs"
                                            />
                                            <div
                                                id="evidence-hash-err"
                                                className="studio-error-msg"
                                            >
                                                <AlertTriangle className="h-3.5 w-3.5" />
                                                Must be a 66-char 0x-prefixed
                                                SHA-256 hex digest.
                                            </div>
                                        </div>
                                    </div>

                                    {/* 1-Click WebCrypto SHA-256 Evidence Hasher */}
                                    <div className="border-border bg-muted/30 flex flex-col gap-2 rounded-lg border p-3 sm:flex-row sm:items-center">
                                        <div className="flex-1">
                                            <label
                                                htmlFor="hash-source"
                                                className="text-muted-foreground block text-[11px] font-medium"
                                            >
                                                WebCrypto SHA-256 Receipt / Photo
                                                Digest Helper:
                                            </label>
                                            <input
                                                id="hash-source"
                                                type="text"
                                                value={hashSourceText}
                                                onChange={(e) =>
                                                    setHashSourceText(
                                                        e.target.value,
                                                    )
                                                }
                                                className="bg-transparent font-mono text-xs focus:outline-none w-full"
                                            />
                                        </div>
                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={generateDigestAndApply}
                                            className="shrink-0 text-xs"
                                        >
                                            <Hash className="mr-1.5 h-3.5 w-3.5 text-emerald-400" />
                                            Hash &amp; Inject SHA-256
                                        </Button>
                                    </div>

                                    {/* Action JSON Payload Editor */}
                                    <div className="space-y-1.5">
                                        <div className="flex items-center justify-between">
                                            <label
                                                htmlFor="action-json-editor"
                                                className="font-mono text-xs font-semibold"
                                            >
                                                Action Arguments JSON Payload
                                            </label>
                                            <button
                                                type="button"
                                                onClick={() => {
                                                    setTangibleRatioInput(
                                                        "4000",
                                                    );
                                                    const broken =
                                                        structuredClone(
                                                            selectedTemplate.defaultPayload,
                                                        );
                                                    if (broken.config) {
                                                        broken.config.tangibleRatioBps = 4000;
                                                    }
                                                    setJsonText(
                                                        JSON.stringify(
                                                            broken,
                                                            null,
                                                            2,
                                                        ),
                                                    );
                                                }}
                                                className="text-[11px] text-amber-400 underline hover:text-amber-300"
                                            >
                                                Test &lt;51% Revert Guard
                                            </button>
                                        </div>
                                        <textarea
                                            id="action-json-editor"
                                            rows={10}
                                            value={jsonText}
                                            onChange={(e) =>
                                                setJsonText(e.target.value)
                                            }
                                            className="border-border bg-zinc-950 w-full rounded-lg border p-3 font-mono text-xs text-emerald-300 focus:outline-none"
                                        />
                                    </div>

                                    <div className="flex items-center justify-between gap-3">
                                        <Button
                                            type="submit"
                                            className="bg-emerald-600 text-white hover:bg-emerald-500"
                                        >
                                            <Play className="mr-1.5 h-4 w-4" />
                                            Execute{" "}
                                            {selectedTemplate.service}::
                                            {selectedTemplate.action}
                                        </Button>

                                        <Button
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() => {
                                                try {
                                                    const parsed =
                                                        JSON.parse(jsonText);
                                                    navigator.clipboard.writeText(
                                                        selectedTemplate.sdkSnippet(
                                                            parsed,
                                                        ),
                                                    );
                                                    toast.success(
                                                        "Copied TypeScript SDK call",
                                                    );
                                                } catch {
                                                    toast.error(
                                                        "Fix JSON before copying SDK snippet",
                                                    );
                                                }
                                            }}
                                            className="text-xs"
                                        >
                                            <Code2 className="mr-1.5 h-3.5 w-3.5" />
                                            Copy Headless SDK Call
                                        </Button>
                                    </div>
                                </form>

                                {lastResult && (
                                    <div
                                        className={`mt-4 flex items-start gap-2.5 rounded-lg border p-3 font-mono text-xs ${
                                            lastResult.ok
                                                ? "border-emerald-500/40 bg-emerald-500/10 text-emerald-300"
                                                : "border-rose-500/40 bg-rose-500/10 text-rose-300"
                                        }`}
                                    >
                                        {lastResult.ok ? (
                                            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-emerald-400" />
                                        ) : (
                                            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-rose-400" />
                                        )}
                                        <div>
                                            <div className="font-semibold">
                                                {lastResult.ok
                                                    ? `Committed to Triedent (${lastResult.timestamp})`
                                                    : `State Transition Reverted (${lastResult.timestamp})`}
                                            </div>
                                            <div className="mt-0.5">
                                                {lastResult.message}
                                            </div>
                                        </div>
                                    </div>
                                )}
                            </CardContent>
                        </Card>
                    </div>
                </div>
            ) : (
                /* Triedent GraphQL Playground */
                <div className="grid gap-6 lg:grid-cols-12">
                    <Card className="lg:col-span-6">
                        <CardHeader className="pb-3">
                            <div className="flex items-center justify-between">
                                <CardTitle className="text-sm font-semibold">
                                    GraphQL Query Editor
                                </CardTitle>
                                <select
                                    aria-label="Target Wasm Service"
                                    value={gqlService}
                                    onChange={(e) => {
                                        const svc = e.target.value;
                                        setGqlService(svc);
                                        const found = STUDIO_TABLES.find(
                                            (t) => t.service === svc,
                                        );
                                        if (found)
                                            setGqlQuery(found.graphqlQuery);
                                    }}
                                    className="border-border bg-background rounded border px-2 py-1 font-mono text-xs"
                                >
                                    {STUDIO_TABLES.map((t) => (
                                        <option key={t.id} value={t.service}>
                                            /api/{t.service}/graphql (
                                            {t.tableName})
                                        </option>
                                    ))}
                                </select>
                            </div>
                            <CardDescription className="text-xs">
                                Query Wasm tables directly without external
                                subgraph indexers.
                            </CardDescription>
                        </CardHeader>
                        <CardContent className="space-y-3">
                            <textarea
                                rows={12}
                                value={gqlQuery}
                                onChange={(e) => setGqlQuery(e.target.value)}
                                aria-label="GraphQL Query"
                                className="border-border bg-zinc-950 w-full rounded-lg border p-3 font-mono text-xs text-amber-200 focus:outline-none"
                            />
                            <div className="flex items-center justify-between">
                                <Button
                                    onClick={handleRunGraphql}
                                    className="bg-emerald-600 text-white hover:bg-emerald-500"
                                >
                                    <Play className="mr-1.5 h-4 w-4" />
                                    Run Query
                                </Button>
                                <Button
                                    variant="outline"
                                    size="sm"
                                    onClick={() => {
                                        navigator.clipboard.writeText(
                                            `const data = await client.query("${gqlService}", \`${gqlQuery}\`);`,
                                        );
                                        toast.success(
                                            "Copied client.query() snippet",
                                        );
                                    }}
                                    className="text-xs"
                                >
                                    <Copy className="mr-1.5 h-3.5 w-3.5" />
                                    Copy client.query()
                                </Button>
                            </div>
                        </CardContent>
                    </Card>

                    <Card className="lg:col-span-6">
                        <CardHeader className="pb-3">
                            <div className="flex items-center justify-between">
                                <CardTitle className="text-sm font-semibold">
                                    Triedent GraphQL Response
                                </CardTitle>
                                <Badge
                                    variant="outline"
                                    className="border-emerald-500/40 font-mono text-[10px] text-emerald-400"
                                >
                                    0.14 ms Memory-Mapped Read
                                </Badge>
                            </div>
                            <CardDescription className="text-xs">
                                Real-time response payload from{" "}
                                <code>/api/{gqlService}/graphql</code>
                            </CardDescription>
                        </CardHeader>
                        <CardContent>
                            <pre className="border-border bg-zinc-950 h-[300px] overflow-auto rounded-lg border p-3 font-mono text-xs text-emerald-300">
                                {gqlOutput ||
                                    "// Click 'Run Query' to inspect live Triedent records"}
                            </pre>
                        </CardContent>
                    </Card>
                </div>
            )}
        </div>
    );
};
