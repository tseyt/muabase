/**
 * Canonical Muabase Wasm service account slugs matching `#[psibase::service(name = ...)]`.
 * Single source of truth used across SDK transport, namespaces, and tests to prevent slug drift.
 */
export const MUABASE_SERVICES = {
  TOKENS: "tokens",
  LEASE_ENGINE: "lease-eng",
  STAGED_TX: "staged-tx",
  EVALUATIONS: "evaluation",
  CLAIM_COUNCIL: "claim-coun",
  EXTERNAL_VAULT: "ext-vault",
  CREDIT_CLEAR: "cred-clear",
  WAQF_TREASURY: "waqf-treas",
  MUAMALAT_TAHKIM: "muamalat",
  TOKEN_STREAM: "tok-stream",
  GUILDS: "guilds",
  CREDENTIALS: "credential",
  PRODUCERS: "producers",
  AUTH_DELEGATE: "auth-delg",
} as const;

export type MuabaseServiceSlug = (typeof MUABASE_SERVICES)[keyof typeof MUABASE_SERVICES];
