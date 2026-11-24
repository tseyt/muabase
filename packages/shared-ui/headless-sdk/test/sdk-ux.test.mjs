import test from "node:test";
import assert from "node:assert/strict";
import { createMuabaseClient } from "../dist/client.js";

test("GIVEN subdomain transport with a raw IP endpoint WHEN resolving GraphQL URL THEN an actionable error is thrown instead of an unresolvable host", () => {
  const client = createMuabaseClient({
    endpoint: "http://127.0.0.1:8080",
    transport: "subdomain",
  });
  assert.throws(
    () => client.resolveGraphqlUrl("ext-vault"),
    /muabase\.localhost:8080/,
  );

  const localHostClient = createMuabaseClient({
    endpoint: "http://muabase.localhost:8080",
    transport: "subdomain",
  });
  assert.equal(
    localHostClient.resolveGraphqlUrl("ext-vault"),
    "http://ext-vault.muabase.localhost:8080/graphql",
  );
});

test("GIVEN domain helper actions WHEN invoked THEN canonical Wasm slugs, integer amounts, and cent-conserving splits are emitted", async () => {
  const calls = [];
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async (url, init) => {
    calls.push({ url: String(url), body: JSON.parse(String(init?.body ?? "{}")) });
    return new Response(JSON.stringify({ leaseId: 77, data: { terminologyLocale: "muamalat" } }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  };

  try {
    const client = createMuabaseClient({ endpoint: "https://node.muabase.org", account: "alice" });

    await client.guild.claims.submitEvidence({
      leaseId: "9",
      claimedAmountUsd: 1.14,
      preConditionEvidenceHash: "0x" + "a".repeat(64),
      postConditionEvidenceHash: "0x" + "b".repeat(64),
      repairQuoteHash: "0x" + "c".repeat(64),
    });
    assert.equal(calls[0].url, "https://node.muabase.org/api/claim-coun/action/submitDamageClaim");
    assert.equal(calls[0].body.claimed_amount.value, 11400);

    await client.guild.credit.netInvoices("10", "20");
    assert.equal(calls[1].url, "https://node.muabase.org/api/cred-clear/action/executeBilateralNetting");
    assert.equal(calls[1].body.invoice_1_id, 10);

    const booking = await client.dining.supperClubs.book({
      clubId: 5,
      seats: 1,
      pricePerSeatCents: 101,
    });
    assert.equal(
      booking.splits.hostTakeHomeCents +
        booking.splits.mutualShieldReserveCents +
        booking.splits.platformFeeCents,
      101,
    );

    const locale = await client.branding.getTerminologyLocale();
    assert.equal(locale, "muamalat");
  } finally {
    globalThis.fetch = originalFetch;
  }
});
