export type MuabaseStreamTransport = "auto" | "sse" | "ws" | "poll";

/**
 * Normalizes direct GraphQL table arrays (`remittanceAttestationTable`, `remittanceIntentTable`)
 * as well as Relay-style paginated connections (`{ edges: [{ node }] }`) into a typed row array.
 */
export function extractTableRows<T = any>(res: any, table: string): T[] {
  if (!res) {
    return [];
  }
  if (Array.isArray(res)) {
    return res;
  }
  const candidate = res[table] ?? Object.values(res)[0];
  if (Array.isArray(candidate)) {
    return candidate;
  }
  if (candidate && Array.isArray(candidate.edges)) {
    return candidate.edges.map((edge: { node: T }) => edge.node);
  }
  if (candidate && typeof candidate === "object") {
    return [candidate as T];
  }
  return [];
}

/**
 * Builds the canonical `psinode` Server-Sent Events (SSE) subscription URL for a Wasm service table.
 */
export function buildTableSseUrl(endpoint: string, service: string, table: string): string {
  const base = endpoint.replace(/\/+$/, "");
  return `${base}/events/sse?service=${encodeURIComponent(service)}&table=${encodeURIComponent(table)}`;
}

/**
 * Builds the canonical `psinode` WebSocket subscription URL (`http` -> `ws`, `https` -> `wss`)
 * for a Wasm service table.
 */
export function buildTableWsUrl(endpoint: string, service: string, table: string): string {
  const base = endpoint.replace(/\/+$/, "").replace(/^http(s?):\/\//i, "ws$1://");
  return `${base}/events?service=${encodeURIComponent(service)}&table=${encodeURIComponent(table)}`;
}
