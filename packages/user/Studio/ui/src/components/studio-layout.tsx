import React, { useState } from "react";
import { NavLink, Outlet } from "react-router-dom";
import {
    Activity,
    Code2,
    Database,
    Globe,
    Layers,
    Play,
    RotateCcw,
    ShieldCheck,
    Sparkles,
    Terminal,
    UserCheck,
    Zap,
} from "lucide-react";

import { Badge } from "@shared/shadcn/ui/badge";
import { Button } from "@shared/shadcn/ui/button";

import { useStudio } from "../context/studio-context";
import { STUDIO_ACCOUNTS, STUDIO_TABLES } from "../lib/studio-data";

export const StudioLayout: React.FC = () => {
    const {
        endpoint,
        setEndpoint,
        locale,
        toggleLocale,
        activeAccount,
        setActiveAccountByName,
        tables,
        events,
        resetStudioState,
    } = useStudio();

    const [showEventDrawer, setShowEventDrawer] = useState<boolean>(true);
    const [editingEndpoint, setEditingEndpoint] = useState<boolean>(false);

    const totalRows = Object.values(tables).reduce(
        (acc, rows) => acc + rows.length,
        0,
    );

    const navItems = [
        {
            to: "/",
            label: "Studio Overview",
            icon: <Layers className="h-4 w-4" />,
            badge: "9 Services",
        },
        {
            to: "/tables",
            label: "Table Explorer",
            icon: <Database className="h-4 w-4" />,
            badge: `${STUDIO_TABLES.length} Tables`,
        },
        {
            to: "/actions",
            label: "Actions & GraphQL",
            icon: <Play className="h-4 w-4" />,
            badge: "Reducers",
        },
        {
            to: "/invariants",
            label: "Invariants Studio",
            icon: <ShieldCheck className="h-4 w-4" />,
            badge: "93/4/3 & Floor",
        },
        {
            to: "/sdk",
            label: "Headless SDK & Locales",
            icon: <Code2 className="h-4 w-4" />,
            badge: "TypeScript",
        },
    ];

    return (
        <div className="bg-background text-foreground flex h-svh w-full flex-col overflow-hidden">
            {/* Top Supabase / SpacetimeDB Telemetry Bar */}
            <header className="border-border bg-card/80 flex h-14 shrink-0 items-center justify-between border-b px-4 backdrop-blur">
                <div className="flex items-center gap-3">
                    <div className="flex items-center gap-2.5">
                        <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-emerald-500/15 text-emerald-400 ring-1 ring-emerald-500/30">
                            <Zap className="h-4 w-4" />
                        </div>
                        <div>
                            <div className="flex items-center gap-2">
                                <span className="text-sm font-bold tracking-tight">
                                    Muabase Studio
                                </span>
                                <Badge
                                    variant="outline"
                                    className="border-emerald-500/40 bg-emerald-500/10 text-[10px] text-emerald-400"
                                >
                                    Triedent Wasm DB
                                </Badge>
                            </div>
                            <p className="text-muted-foreground hidden text-[11px] sm:block">
                                Sub-ms Wasm-in-Database &bull; Zero Backend Middleware
                            </p>
                        </div>
                    </div>

                    <div className="bg-border mx-2 hidden h-6 w-px lg:block" />

                    {/* Live RPC Endpoint Pill */}
                    <div className="hidden items-center gap-2 lg:flex">
                        <span className="flex h-2 w-2 rounded-full bg-emerald-400 shadow-[0_0_8px_#34d399]" />
                        {editingEndpoint ? (
                            <input
                                type="url"
                                value={endpoint}
                                onChange={(e) => setEndpoint(e.target.value)}
                                onBlur={() => setEditingEndpoint(false)}
                                onKeyDown={(e) =>
                                    e.key === "Enter" && setEditingEndpoint(false)
                                }
                                autoFocus
                                className="border-border bg-background rounded border px-2 py-0.5 font-mono text-xs"
                            />
                        ) : (
                            <button
                                type="button"
                                onClick={() => setEditingEndpoint(true)}
                                className="text-muted-foreground hover:text-foreground font-mono text-xs transition-colors"
                                title="Click to change psinode RPC endpoint"
                            >
                                {endpoint}
                            </button>
                        )}
                        <Badge variant="secondary" className="font-mono text-[10px]">
                            0.18 ms
                        </Badge>
                    </div>
                </div>

                {/* Right Controls: Account Switcher, Dual-Locale Engine, Reset */}
                <div className="flex items-center gap-2.5">
                    {/* Soulbound Account Selector */}
                    <div className="border-border bg-muted/40 hidden items-center gap-2 rounded-md border px-2.5 py-1 md:flex">
                        <UserCheck className="h-3.5 w-3.5 text-emerald-400" />
                        <select
                            aria-label="Active Studio Account"
                            value={activeAccount.account}
                            onChange={(e) =>
                                setActiveAccountByName(e.target.value)
                            }
                            className="bg-transparent font-mono text-xs font-medium focus:outline-none"
                        >
                            {STUDIO_ACCOUNTS.map((acc) => (
                                <option
                                    key={acc.account}
                                    value={acc.account}
                                    className="bg-zinc-900 text-white"
                                >
                                    @{acc.account} ({(acc.trustScoreBps / 100).toFixed(1)}% trust)
                                </option>
                            ))}
                        </select>
                        <Badge
                            variant="outline"
                            className="border-blue-500/30 bg-blue-500/10 font-mono text-[10px] text-blue-300"
                        >
                            {activeAccount.domainBadges[0]}
                        </Badge>
                    </div>

                    {/* Dual-Locale Engine Switcher ("institutional" <-> "muamalat") */}
                    <Button
                        variant="outline"
                        size="sm"
                        onClick={toggleLocale}
                        className="gap-1.5 border-amber-500/30 bg-amber-500/10 text-xs text-amber-300 hover:bg-amber-500/20"
                        title="Toggle Branding Dual-Locale Engine ('institutional' vs 'muamalat')"
                    >
                        <Globe className="h-3.5 w-3.5" />
                        <span>Locale:</span>
                        <span className="font-mono font-semibold">
                            {locale === "institutional"
                                ? "institutional"
                                : "muamalat"}
                        </span>
                    </Button>

                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={resetStudioState}
                        title="Reset Triedent demo tables to initial seed state"
                        className="text-muted-foreground hover:text-foreground h-8 px-2"
                    >
                        <RotateCcw className="h-3.5 w-3.5" />
                    </Button>
                </div>
            </header>

            {/* Main Studio Workspace */}
            <div className="flex min-h-0 flex-1 overflow-hidden">
                {/* Left Sidebar Navigation */}
                <aside className="border-border bg-card/40 flex w-60 shrink-0 flex-col justify-between border-r p-3">
                    <div className="space-y-4">
                        <div className="space-y-1">
                            <p className="text-muted-foreground px-2.5 text-[11px] font-semibold tracking-wider uppercase">
                                Console Modules
                            </p>
                            <nav className="space-y-1" aria-label="Studio Navigation">
                                {navItems.map((item) => (
                                    <NavLink
                                        key={item.to}
                                        to={item.to}
                                        end={item.to === "/"}
                                        className={({ isActive }) =>
                                            `flex items-center justify-between rounded-md px-2.5 py-2 text-xs font-medium transition-colors ${
                                                isActive
                                                    ? "bg-emerald-500/15 text-emerald-400 ring-1 ring-emerald-500/30"
                                                    : "text-muted-foreground hover:bg-muted/60 hover:text-foreground"
                                            }`
                                        }
                                    >
                                        <span className="flex items-center gap-2.5">
                                            {item.icon}
                                            {item.label}
                                        </span>
                                        <span className="bg-muted text-muted-foreground rounded px-1.5 py-0.5 font-mono text-[10px]">
                                            {item.badge}
                                        </span>
                                    </NavLink>
                                ))}
                            </nav>
                        </div>

                        {/* Quick Canonical Invariants Summary Card */}
                        <div className="border-border bg-muted/25 space-y-2 rounded-lg border p-3">
                            <div className="flex items-center justify-between">
                                <span className="flex items-center gap-1.5 text-xs font-semibold">
                                    <Sparkles className="h-3.5 w-3.5 text-emerald-400" />
                                    Protocol Guards
                                </span>
                                <Badge
                                    variant="outline"
                                    className="border-emerald-500/30 text-[10px] text-emerald-400"
                                >
                                    Active
                                </Badge>
                            </div>
                            <ul className="text-muted-foreground space-y-1.5 font-mono text-[11px]">
                                <li className="flex items-center justify-between">
                                    <span>Fee Split:</span>
                                    <span className="text-foreground font-semibold">
                                        93% / 4% / 3%
                                    </span>
                                </li>
                                <li className="flex items-center justify-between">
                                    <span>Tangible Floor:</span>
                                    <span className="text-emerald-400 font-semibold">
                                        &ge; 5100 bps
                                    </span>
                                </li>
                                <li className="flex items-center justify-between">
                                    <span>Witness Gate:</span>
                                    <span className="text-foreground font-semibold">
                                        Quorum &ge; 2
                                    </span>
                                </li>
                                <li className="flex items-center justify-between">
                                    <span>Charity Sink:</span>
                                    <span className="text-amber-300">
                                        waqf-sadaqah
                                    </span>
                                </li>
                            </ul>
                        </div>
                    </div>

                    {/* Regulatory Safe Harbor Footer */}
                    <div className="border-border bg-muted/20 space-y-1.5 rounded-lg border p-2.5 text-[11px]">
                        <div className="flex items-center justify-between font-medium">
                            <span className="flex items-center gap-1.5 text-emerald-400">
                                <ShieldCheck className="h-3.5 w-3.5" />0 Operator Keys
                            </span>
                            <span className="font-mono text-[10px]">
                                {totalRows} rows
                            </span>
                        </div>
                        <p className="text-muted-foreground leading-tight">
                            NYDFS BitLicense{" "}
                            <code className="text-foreground">
                                23 NYCRR § 200.2(q)
                            </code>{" "}
                            pure software safe harbor.
                        </p>
                    </div>
                </aside>

                {/* Center Content + Bottom Live Reducer Log */}
                <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
                    <main className="min-h-0 flex-1 overflow-y-auto p-6">
                        <Outlet />
                    </main>

                    {/* SpacetimeDB-style Live Reducer & Event Stream Footer */}
                    <footer className="border-border bg-card/70 shrink-0 border-t">
                        <div className="flex items-center justify-between px-4 py-1.5">
                            <button
                                type="button"
                                onClick={() =>
                                    setShowEventDrawer((prev) => !prev)
                                }
                                className="flex items-center gap-2 text-xs font-medium"
                            >
                                <Terminal className="h-3.5 w-3.5 text-emerald-400" />
                                <span>
                                    Live Wasm Reducer &amp; SSE Event Stream
                                </span>
                                <Badge
                                    variant="secondary"
                                    className="font-mono text-[10px]"
                                >
                                    {events.length} events
                                </Badge>
                            </button>
                            <div className="flex items-center gap-3 text-[11px]">
                                <span className="text-muted-foreground flex items-center gap-1">
                                    <Activity className="h-3 w-3 text-emerald-400" />
                                    useMuabaseTable WebSocket: Connected
                                </span>
                                <button
                                    type="button"
                                    onClick={() =>
                                        setShowEventDrawer((prev) => !prev)
                                    }
                                    className="text-muted-foreground hover:text-foreground text-xs"
                                >
                                    {showEventDrawer ? "Hide" : "Expand"}
                                </button>
                            </div>
                        </div>

                        {showEventDrawer && (
                            <div className="border-border max-h-32 overflow-y-auto border-t bg-zinc-950/60 px-4 py-2 font-mono text-xs">
                                {events.map((evt) => (
                                    <div
                                        key={evt.id}
                                        className="flex flex-wrap items-center justify-between gap-2 border-b border-white/5 py-1 last:border-b-0"
                                    >
                                        <div className="flex items-center gap-2">
                                            <span
                                                className={`inline-block h-2 w-2 rounded-full ${
                                                    evt.status === "success"
                                                        ? "bg-emerald-400"
                                                        : "bg-rose-500"
                                                }`}
                                            />
                                            <span className="text-muted-foreground text-[11px]">
                                                {evt.timestamp}
                                            </span>
                                            <Badge
                                                variant="outline"
                                                className="px-1.5 py-0 font-mono text-[10px]"
                                            >
                                                {evt.service}::{evt.action}
                                            </Badge>
                                            <span
                                                className={
                                                    evt.status === "reverted"
                                                        ? "text-rose-400"
                                                        : "text-zinc-200"
                                                }
                                            >
                                                {evt.summary}
                                            </span>
                                        </div>
                                        <div className="text-muted-foreground flex items-center gap-2 text-[11px]">
                                            <span>@{evt.account}</span>
                                            <span className="text-emerald-400">
                                                {evt.latencyMs} ms
                                            </span>
                                        </div>
                                    </div>
                                ))}
                            </div>
                        )}
                    </footer>
                </div>
            </div>
        </div>
    );
};
