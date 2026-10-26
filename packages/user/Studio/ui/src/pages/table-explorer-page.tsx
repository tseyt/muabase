import React, { useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { toast } from "sonner";
import {
    Activity,
    Copy,
    Database,
    Eye,
    Filter,
    Play,
    Plus,
    Search,
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

import { useStudio } from "../context/studio-context";
import { STUDIO_TABLES, StudioTableDefinition } from "../lib/studio-data";

function formatCellValue(
    value: any,
    type: StudioTableDefinition["columns"][number]["type"],
): React.ReactNode {
    if (value === undefined || value === null) {
        return <span className="text-muted-foreground italic">null</span>;
    }
    switch (type) {
        case "bps":
            return (
                <span className="font-mono text-emerald-400">
                    {Number(value)} bps ({(Number(value) / 100).toFixed(2)}%)
                </span>
            );
        case "usd4":
            return (
                <span className="font-mono font-medium">
                    $
                    {(Number(value) / 10000).toLocaleString(undefined, {
                        minimumFractionDigits: 2,
                        maximumFractionDigits: 2,
                    })}
                </span>
            );
        case "cents":
            return (
                <span className="font-mono font-medium">
                    $
                    {(Number(value) / 100).toLocaleString(undefined, {
                        minimumFractionDigits: 2,
                        maximumFractionDigits: 2,
                    })}
                </span>
            );
        case "hash": {
            const str = String(value);
            const short =
                str.length > 18
                    ? `${str.slice(0, 10)}…${str.slice(-6)}`
                    : str;
            return (
                <code
                    title={str}
                    className="bg-muted rounded px-1.5 py-0.5 font-mono text-[11px] text-amber-300"
                >
                    {short}
                </code>
            );
        }
        case "badge":
            return (
                <Badge variant="outline" className="font-mono text-[11px]">
                    {String(value)}
                </Badge>
            );
        case "boolean":
            return value ? (
                <Badge className="bg-emerald-500/20 text-[10px] text-emerald-300">
                    Locked (True)
                </Badge>
            ) : (
                <Badge variant="secondary" className="text-[10px]">
                    False
                </Badge>
            );
        default:
            return <span>{String(value)}</span>;
    }
}

export const TableExplorerPage: React.FC = () => {
    const navigate = useNavigate();
    const [searchParams, setSearchParams] = useSearchParams();
    const { tables, term } = useStudio();

    const selectedTableId =
        searchParams.get("table") || STUDIO_TABLES[0].id;
    const selectedTable = useMemo(
        () =>
            STUDIO_TABLES.find((t) => t.id === selectedTableId) ??
            STUDIO_TABLES[0],
        [selectedTableId],
    );

    const [filterText, setFilterText] = useState<string>("");
    const [inspectedRow, setInspectedRow] = useState<Record<string, any> | null>(
        null,
    );

    const rows = tables[selectedTable.id] || [];

    const filteredRows = useMemo(() => {
        if (!filterText.trim()) return rows;
        const q = filterText.toLowerCase();
        return rows.filter((row) =>
            Object.values(row).some((val) =>
                String(val).toLowerCase().includes(q),
            ),
        );
    }, [rows, filterText]);

    // Build CSS custom properties --label-2, --label-3, etc. for @container stacked responsive-table
    const tableCssVars = useMemo(() => {
        const vars: Record<string, string> = {};
        selectedTable.columns.forEach((col, idx) => {
            vars[`--label-${idx + 1}`] = `"${col.label}"`;
        });
        return vars as React.CSSProperties;
    }, [selectedTable]);

    const copyHookSnippet = () => {
        const snippet = `const { data, loading, error } = useMuabaseTable({
  client,
  service: "${selectedTable.service}",
  table: "${selectedTable.tableName}",
  query: \`${selectedTable.graphqlQuery.replace(/\s+/g, " ").trim()}\`,
  pollIntervalMs: 2000,
});`;
        navigator.clipboard.writeText(snippet);
        toast.success("Copied useMuabaseTable() hook to clipboard");
    };

    return (
        <div className="mx-auto max-w-7xl space-y-6">
            {/* Header */}
            <div className="flex flex-col justify-between gap-4 sm:flex-row sm:items-center">
                <div>
                    <h1 className="flex items-center gap-2 text-2xl font-bold tracking-tight">
                        <Database className="h-6 w-6 text-emerald-400" />
                        Triedent Real-Time Table Explorer
                    </h1>
                    <p className="text-muted-foreground text-xs">
                        Browse memory-mapped Wasm service tables with live{" "}
                        <code className="text-foreground">useMuabaseTable</code>{" "}
                        WebSocket synchronization.
                    </p>
                </div>
                <div className="flex items-center gap-2">
                    <Button
                        variant="outline"
                        size="sm"
                        onClick={copyHookSnippet}
                        className="text-xs"
                    >
                        <Copy className="mr-1.5 h-3.5 w-3.5" />
                        Copy React Hook
                    </Button>
                    <Button
                        size="sm"
                        onClick={() =>
                            navigate(
                                `/actions?service=${selectedTable.service}`,
                            )
                        }
                        className="bg-emerald-600 text-white hover:bg-emerald-500"
                    >
                        <Plus className="mr-1.5 h-3.5 w-3.5" />
                        Run Action on &quot;{selectedTable.service}&quot;
                    </Button>
                </div>
            </div>

            <div className="grid gap-6 lg:grid-cols-12">
                {/* Left Table Selector Sidebar */}
                <div className="space-y-2 lg:col-span-3">
                    <p className="text-muted-foreground px-1 text-xs font-semibold uppercase">
                        Wasm Service Tables
                    </p>
                    <div className="space-y-1.5">
                        {STUDIO_TABLES.map((tbl) => {
                            const active = tbl.id === selectedTable.id;
                            const count = (tables[tbl.id] || []).length;
                            return (
                                <button
                                    key={tbl.id}
                                    type="button"
                                    onClick={() => {
                                        setSearchParams({ table: tbl.id });
                                        setInspectedRow(null);
                                    }}
                                    className={`flex w-full flex-col rounded-lg border p-2.5 text-left transition-colors ${
                                        active
                                            ? "border-emerald-500/50 bg-emerald-500/10"
                                            : "border-border bg-card/50 hover:bg-muted/50"
                                    }`}
                                >
                                    <div className="flex items-center justify-between">
                                        <span className="font-mono text-xs font-semibold">
                                            {tbl.tableName}
                                        </span>
                                        <Badge
                                            variant="secondary"
                                            className="font-mono text-[10px]"
                                        >
                                            {count}
                                        </Badge>
                                    </div>
                                    <div className="mt-1 flex items-center justify-between text-[11px]">
                                        <span className="text-emerald-400 font-mono">
                                            {tbl.service}
                                        </span>
                                        <span className="text-muted-foreground truncate pl-2">
                                            {term(tbl.canonicalKey)}
                                        </span>
                                    </div>
                                </button>
                            );
                        })}
                    </div>
                </div>

                {/* Right Table Data Grid & Inspector */}
                <div className="space-y-4 lg:col-span-9">
                    <Card>
                        <CardHeader className="pb-3">
                            <div className="flex flex-col justify-between gap-3 sm:flex-row sm:items-center">
                                <div>
                                    <div className="flex items-center gap-2">
                                        <CardTitle className="font-mono text-lg">
                                            {selectedTable.tableName}
                                        </CardTitle>
                                        <Badge
                                            variant="outline"
                                            className="border-emerald-500/40 font-mono text-xs text-emerald-400"
                                        >
                                            service: &quot;{selectedTable.service}&quot;
                                        </Badge>
                                        <Badge
                                            variant="secondary"
                                            className="text-xs"
                                        >
                                            {term(selectedTable.canonicalKey)}
                                        </Badge>
                                    </div>
                                    <CardDescription className="mt-1 text-xs">
                                        {selectedTable.description}
                                    </CardDescription>
                                </div>

                                {/* Search Filter Input */}
                                <div className="relative w-full sm:w-64">
                                    <Search className="text-muted-foreground absolute top-2.5 left-2.5 h-3.5 w-3.5" />
                                    <input
                                        type="search"
                                        placeholder="Filter table rows..."
                                        value={filterText}
                                        onChange={(e) =>
                                            setFilterText(e.target.value)
                                        }
                                        aria-label="Filter table rows"
                                        className="border-border bg-background w-full rounded-md border py-1.5 pr-3 pl-8 text-xs focus:outline-none"
                                    />
                                </div>
                            </div>
                        </CardHeader>

                        <CardContent className="space-y-4">
                            {/* Modern Web Guidance: responsive-table with role="region", tabIndex={0}, aria-labelledby, sticky headers, and @container stacked mode */}
                            <div
                                tabIndex={0}
                                role="region"
                                aria-labelledby="studio-table-caption"
                                className="studio-table-wrapper border-border rounded-lg border"
                            >
                                <table
                                    style={tableCssVars}
                                    className="text-left text-xs"
                                >
                                    <caption
                                        id="studio-table-caption"
                                        className="sr-only"
                                    >
                                        {selectedTable.tableName} in Wasm service{" "}
                                        {selectedTable.service}
                                    </caption>
                                    <thead className="bg-muted/90 text-muted-foreground border-b backdrop-blur">
                                        <tr>
                                            {selectedTable.columns.map((col) => (
                                                <th
                                                    key={col.key}
                                                    scope="col"
                                                    className="border-border border-b px-3.5 py-2.5 font-mono text-[11px] font-semibold whitespace-nowrap"
                                                >
                                                    {col.label}
                                                </th>
                                            ))}
                                            <th
                                                scope="col"
                                                className="border-border border-b px-3.5 py-2.5 text-right font-mono text-[11px] font-semibold"
                                            >
                                                Inspect
                                            </th>
                                        </tr>
                                    </thead>
                                    <tbody className="divide-border divide-y">
                                        {filteredRows.length === 0 ? (
                                            <tr>
                                                <td
                                                    colSpan={
                                                        selectedTable.columns
                                                            .length + 1
                                                    }
                                                    className="text-muted-foreground py-8 text-center text-xs"
                                                >
                                                    No matching rows found in{" "}
                                                    {selectedTable.tableName}.
                                                </td>
                                            </tr>
                                        ) : (
                                            filteredRows.map((row, rIdx) => {
                                                const pkVal =
                                                    row[
                                                        selectedTable.primaryKey
                                                    ] ?? rIdx;
                                                const firstCol =
                                                    selectedTable.columns[0];
                                                const restCols =
                                                    selectedTable.columns.slice(
                                                        1,
                                                    );
                                                return (
                                                    <tr
                                                        key={String(pkVal)}
                                                        onClick={() =>
                                                            setInspectedRow(row)
                                                        }
                                                        className="hover:bg-muted/40 cursor-pointer transition-colors"
                                                    >
                                                        {/* Sticky row header (<th>) per responsive-table guidance */}
                                                        <th
                                                            scope="row"
                                                            className="border-border border-r px-3.5 py-2.5 font-mono font-semibold whitespace-nowrap text-emerald-400"
                                                        >
                                                            #
                                                            {String(
                                                                row[
                                                                    firstCol.key
                                                                ],
                                                            )}
                                                        </th>
                                                        {restCols.map((col) => (
                                                            <td
                                                                key={col.key}
                                                                className="px-3.5 py-2.5 whitespace-nowrap"
                                                            >
                                                                {formatCellValue(
                                                                    row[
                                                                        col.key
                                                                    ],
                                                                    col.type,
                                                                )}
                                                            </td>
                                                        ))}
                                                        <td className="px-3.5 py-2.5 text-right">
                                                            <Button
                                                                variant="ghost"
                                                                size="sm"
                                                                onClick={(
                                                                    e,
                                                                ) => {
                                                                    e.stopPropagation();
                                                                    setInspectedRow(
                                                                        row,
                                                                    );
                                                                }}
                                                                className="h-6 px-2 text-[11px]"
                                                            >
                                                                <Eye className="mr-1 h-3 w-3" />
                                                                JSON
                                                            </Button>
                                                        </td>
                                                    </tr>
                                                );
                                            })
                                        )}
                                    </tbody>
                                </table>
                            </div>

                            {/* Table Footer Metadata */}
                            <div className="text-muted-foreground flex flex-wrap items-center justify-between gap-2 text-xs">
                                <span className="flex items-center gap-1.5">
                                    <Activity className="h-3.5 w-3.5 text-emerald-400" />
                                    Showing {filteredRows.length} of {rows.length}{" "}
                                    rows in{" "}
                                    <code className="text-foreground">
                                        {selectedTable.tableName}
                                    </code>
                                </span>
                                <span className="flex items-center gap-1.5 font-mono text-[11px]">
                                    <Filter className="h-3 w-3" />
                                    Endpoint: /api/{selectedTable.service}/graphql
                                </span>
                            </div>
                        </CardContent>
                    </Card>

                    {/* Row JSON Inspector & GraphQL Subscription Preview */}
                    <div className="grid gap-4 md:grid-cols-2">
                        <Card>
                            <CardHeader className="pb-2">
                                <CardTitle className="text-xs font-semibold uppercase">
                                    Selected Row Inspector (Triedent Leaf JSON)
                                </CardTitle>
                            </CardHeader>
                            <CardContent>
                                <pre className="bg-muted/50 max-h-48 overflow-auto rounded-md p-3 font-mono text-xs text-emerald-300">
                                    {JSON.stringify(
                                        inspectedRow ?? filteredRows[0] ?? {},
                                        null,
                                        2,
                                    )}
                                </pre>
                            </CardContent>
                        </Card>

                        <Card>
                            <CardHeader className="pb-2">
                                <div className="flex items-center justify-between">
                                    <CardTitle className="text-xs font-semibold uppercase">
                                        Live GraphQL Query (
                                        {selectedTable.service})
                                    </CardTitle>
                                    <Button
                                        variant="ghost"
                                        size="sm"
                                        onClick={() =>
                                            navigate(
                                                `/actions?service=${selectedTable.service}&tab=graphql`,
                                            )
                                        }
                                        className="h-6 px-2 text-[11px] text-emerald-400"
                                    >
                                        <Play className="mr-1 h-3 w-3" />
                                        Open in Playground
                                    </Button>
                                </div>
                            </CardHeader>
                            <CardContent>
                                <pre className="bg-muted/50 max-h-48 overflow-auto rounded-md p-3 font-mono text-xs text-amber-200">
                                    {selectedTable.graphqlQuery}
                                </pre>
                            </CardContent>
                        </Card>
                    </div>
                </div>
            </div>
        </div>
    );
};
