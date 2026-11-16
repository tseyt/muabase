import { useState, useEffect, useRef } from "react";
import { MuabaseClient } from "../client";
import {
  MuabaseStreamTransport,
  extractTableRows,
  buildTableSseUrl,
} from "../stream";

export { extractTableRows, buildTableSseUrl };
export type { MuabaseStreamTransport };

export interface UseTableOptions<T> {
  client: MuabaseClient;
  service: string;
  table: string;
  query: string;
  variables?: Record<string, any>;
  filter?: (row: T) => boolean;
  pollIntervalMs?: number;
  /**
   * Live push transport used to trigger immediate table refreshes when `psinode` commits
   * new witness attestations (`RemittanceAttestationTable`) or settlement hashes (`RemittanceIntentTable`).
   * Defaults to `"auto"` (prefers Server-Sent Events `EventSource`, falls back to `WebSocket`, then polling).
   */
  transport?: MuabaseStreamTransport;
}

/**
 * SpacetimeDB-style reactive table sync hook for Muabase.
 * Subscribes to live Server-Sent Events (SSE) or WebSocket event streams from `psinode`,
 * updating attestation quorum status and bridge settlement confirmation hashes in real time.
 */
export function useMuabaseTable<T = any>({
  client,
  service,
  table,
  query,
  variables,
  filter,
  pollIntervalMs = 2000,
  transport = "auto",
}: UseTableOptions<T>) {
  const [data, setData] = useState<T[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<Error | null>(null);

  const filterRef = useRef(filter);
  filterRef.current = filter;

  const variablesKey = variables ? JSON.stringify(variables) : "";

  const fetchTableData = async () => {
    try {
      const res = await client.query(service, query, variables);
      const rows = extractTableRows<T>(res, table);
      const filtered = filterRef.current ? rows.filter(filterRef.current) : rows;
      setData(filtered);
      setError(null);
    } catch (err: any) {
      setError(err instanceof Error ? err : new Error(String(err)));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    let isMounted = true;
    let sse: EventSource | null = null;
    let ws: WebSocket | null = null;

    // Initial fetch
    fetchTableData();

    const connectWebSocket = () => {
      try {
        if (typeof WebSocket === "undefined") {
          return;
        }
        const wsUrl =
          client.endpoint.replace(/^http/, "ws") +
          `/events?service=${encodeURIComponent(service)}&table=${encodeURIComponent(table)}`;
        ws = new WebSocket(wsUrl);
        ws.onmessage = () => {
          if (isMounted) {
            fetchTableData();
          }
        };
        ws.onerror = () => {
          // Polling fallback remains active
        };
      } catch {
        // WebSocket unavailable in current runtime
      }
    };

    // Prefer Server-Sent Events (SSE) when transport is "sse" or "auto"
    if ((transport === "sse" || transport === "auto") && typeof EventSource !== "undefined") {
      try {
        sse = new EventSource(buildTableSseUrl(client.endpoint, service, table));
        sse.onmessage = () => {
          if (isMounted) {
            fetchTableData();
          }
        };
        sse.addEventListener("tableUpdate", () => {
          if (isMounted) {
            fetchTableData();
          }
        });
        sse.onerror = () => {
          if (transport === "auto" && !ws && isMounted) {
            sse?.close();
            sse = null;
            connectWebSocket();
          }
        };
      } catch {
        if (transport === "auto") {
          connectWebSocket();
        }
      }
    } else if (transport === "ws" || transport === "auto") {
      connectWebSocket();
    }

    // Polling fallback / background refresher
    const timer = setInterval(() => {
      if (isMounted) {
        fetchTableData();
      }
    }, pollIntervalMs);

    return () => {
      isMounted = false;
      clearInterval(timer);
      if (sse) {
        sse.close();
      }
      if (ws) {
        ws.close();
      }
    };
  }, [client, service, table, query, variablesKey, pollIntervalMs, transport]);

  return {
    data,
    loading,
    error,
    refresh: fetchTableData,
  };
}
