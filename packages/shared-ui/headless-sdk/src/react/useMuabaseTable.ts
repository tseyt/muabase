import { useState, useEffect, useRef, useCallback } from "react";
import { MuabaseClient } from "../client.js";
import {
  MuabaseStreamTransport,
  extractTableRows,
  buildTableSseUrl,
  buildTableWsUrl,
} from "../stream.js";

export { extractTableRows, buildTableSseUrl, buildTableWsUrl };
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

  const variablesRef = useRef(variables);
  variablesRef.current = variables;

  const mountedRef = useRef(true);
  const requestSeqRef = useRef(0);

  const variablesKey = variables ? JSON.stringify(variables) : "";

  const fetchTableData = useCallback(async () => {
    const seq = ++requestSeqRef.current;
    try {
      const res = await client.query(service, query, variablesRef.current);
      if (!mountedRef.current || seq !== requestSeqRef.current) {
        return;
      }
      const rows = extractTableRows<T>(res, table);
      const filtered = filterRef.current ? rows.filter(filterRef.current) : rows;
      setData(filtered);
      setError(null);
    } catch (err: any) {
      if (!mountedRef.current || seq !== requestSeqRef.current) {
        return;
      }
      setError(err instanceof Error ? err : new Error(String(err)));
    } finally {
      if (mountedRef.current && seq === requestSeqRef.current) {
        setLoading(false);
      }
    }
  }, [client, service, table, query, variablesKey]);

  useEffect(() => {
    mountedRef.current = true;
    let sse: EventSource | null = null;
    let ws: WebSocket | null = null;

    // Initial fetch
    fetchTableData();

    const connectWebSocket = () => {
      try {
        if (typeof WebSocket === "undefined") {
          return;
        }
        ws = new WebSocket(buildTableWsUrl(client.endpoint, service, table));
        ws.onmessage = () => {
          if (mountedRef.current) {
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
          if (mountedRef.current) {
            fetchTableData();
          }
        };
        sse.addEventListener("tableUpdate", () => {
          if (mountedRef.current) {
            fetchTableData();
          }
        });
        sse.onerror = () => {
          if (transport === "auto" && !ws && mountedRef.current) {
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

    // Polling fallback / background refresher (disabled when pollIntervalMs <= 0)
    const timer =
      pollIntervalMs > 0
        ? setInterval(() => {
            if (mountedRef.current) {
              fetchTableData();
            }
          }, pollIntervalMs)
        : null;

    return () => {
      mountedRef.current = false;
      requestSeqRef.current++;
      if (timer !== null) {
        clearInterval(timer);
      }
      if (sse) {
        sse.close();
      }
      if (ws) {
        ws.close();
      }
    };
  }, [client, service, table, fetchTableData, pollIntervalMs, transport]);

  return {
    data,
    loading,
    error,
    refresh: fetchTableData,
  };
}
