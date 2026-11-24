import test from "node:test";
import assert from "node:assert/strict";
import {
  createMuabaseClient,
  assertValidSha256Commitment,
  assertZakatTamlikSplitInvariant,
} from "../dist/client.js";
import { buildTableSseUrl, extractTableRows } from "../dist/stream.js";

const COMMITMENT_HASH =
  "0x9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
const DECLARATION_HASH =
  "0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const LICENSE_HASH =
  "0x4b227777d4dd1fc61c6f884f48641d02b4d121d3fd328cb08b5531fcacdabf8a";

test("Zero-PII commitment guard rejects plaintext PII, short hex, and all-zero hashes", () => {
  assert.equal(assertValidSha256Commitment(COMMITMENT_HASH), COMMITMENT_HASH);

  assert.throws(
    () => assertValidSha256Commitment("Amina Qirimli | +90-555-0100"),
    /PII Guard Violation/,
  );
  assert.throws(() => assertValidSha256Commitment("0x1234abcd"), /PII Guard Violation/);
  assert.throws(
    () => assertValidSha256Commitment("0x" + "0".repeat(64)),
    /all-zero 32-byte digest/,
  );
});

test("ZAKAT_TAMLIK_100 enforces 100% principal conservation and throws on any protocol fee deduction", () => {
  const valid = assertZakatTamlikSplitInvariant({
    attestationId: 7,
    mode: "ZAKAT_TAMLIK_100",
    railType: "evm_safe",
    corridorJurisdiction: "TR",
    recipientBridgeVault: "0xSafeIstanbulBridgeVault000000000000000001",
    infraOpsVault: "0xQirimSadaqahOps000000000000000000000002",
    zakatPrincipalAmount: 100_000,
    sadaqahTipAmount: 2_000,
    protocolFeeDeduction: 0,
    recipientCommitmentHash: COMMITMENT_HASH,
  });
  assert.equal(valid.mode, "ZAKAT_TAMLIK_100");
  assert.equal(valid.zakatPrincipalAmount, 100_000);
  assert.equal(valid.sadaqahTipAmount, 2_000);
  assert.equal(valid.protocolFeeDeduction, 0);

  assert.throws(
    () =>
      assertZakatTamlikSplitInvariant({
        attestationId: 7,
        mode: "ZAKAT_TAMLIK_100",
        railType: "evm_safe",
        corridorJurisdiction: "TR",
        recipientBridgeVault: "0xSafeIstanbulBridgeVault000000000000000001",
        zakatPrincipalAmount: 100_000,
        sadaqahTipAmount: 0,
        protocolFeeDeduction: 1,
        recipientCommitmentHash: COMMITMENT_HASH,
      }),
    /Tamlik Violation/,
  );
});

test("Headless SDK dispatches proposeRemittanceAttestation, attestBeneficiaryCommitment, createZakatRemittanceSplit, and listBridgeCorridors", async () => {
  const calls = [];
  const originalFetch = globalThis.fetch;

  globalThis.fetch = async (url, init) => {
    const body = JSON.parse(String(init?.body ?? "{}"));
    calls.push({ url: String(url), body });

    if (String(url).endsWith("/api/cred-clear/graphql")) {
      return new Response(
        JSON.stringify({
          data: {
            listBridgeCorridors: [
              {
                deskId: 1,
                operator: "bosphorus",
                corridorJurisdiction: "TR",
                deskLabel: "Bosphorus Non-Custodial Sarf",
                licenseRegistryHash: LICENSE_HASH,
                evmSafeAddress: "0x90F8bf6A479f320ead074411a4B0e7944Ea8c9C1",
                btcTaprootDescriptor: "",
                solanaSquadsPda: "",
                fxSpreadBps: 75,
                maxPerTxUsdcCents: 2_500_000,
                sadaqahFeeSubsidySupported: true,
                active: true,
                lastUpdated: 1730000000,
              },
            ],
          },
        }),
        { status: 200, headers: { "Content-Type": "application/json" } },
      );
    }

    return new Response(JSON.stringify({ ok: true, attestationId: 11, intentId: 22 }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    });
  };

  try {
    const client = createMuabaseClient({
      endpoint: "https://qirim.node.muabase.org/",
      locale: "muamalat",
      account: "donor-ali",
    });

    await client.stagedTx.proposeRemittanceAttestation({
      recipientCommitmentHash: COMMITMENT_HASH,
      corridorJurisdiction: "TR",
      nonCommercialPersonalDeclarationHash: DECLARATION_HASH,
      asnafCategory: "FUQARA",
      witnessQuorum: 2,
    });

    await client.stagedTx.attestBeneficiaryCommitment({
      attestationId: 11,
      recipientCommitmentHash: COMMITMENT_HASH,
    });

    const desks = await client.creditClear.listBridgeCorridors({
      corridorJurisdiction: "TR",
      activeOnly: true,
    });
    assert.equal(desks.length, 1);
    assert.equal(desks[0].fxSpreadBps, 75);

    await client.externalVault.createZakatRemittanceSplit({
      attestationId: 11,
      mode: "ZAKAT_TAMLIK_100",
      railType: "evm_safe",
      corridorJurisdiction: "TR",
      recipientBridgeVault: desks[0].evmSafeAddress,
      infraOpsVault: "0xQirimSadaqahOps000000000000000000000002",
      zakatPrincipalAmount: 50_000,
      sadaqahTipAmount: 375,
      recipientCommitmentHash: COMMITMENT_HASH,
    });

    assert.equal(calls.length, 4);
    assert.equal(
      calls[0].url,
      "https://qirim.node.muabase.org/api/staged-tx/action/proposeRemittanceAttestation",
    );
    assert.equal(calls[0].body.asnaf_category, "FUQARA");
    assert.equal(
      calls[1].url,
      "https://qirim.node.muabase.org/api/staged-tx/action/attestBeneficiaryCommitment",
    );
    assert.equal(calls[1].body.domain_badge, "humanitarian-verifier");
    assert.equal(
      calls[3].url,
      "https://qirim.node.muabase.org/api/ext-vault/action/createZakatRemittanceSplit",
    );
    assert.equal(calls[3].body.mode, "ZAKAT_TAMLIK_100");
    assert.equal(calls[3].body.protocol_fee_deduction, 0);
    assert.equal(calls[3].body.zakat_principal_amount, 50_000);
    assert.equal(calls[3].body.sadaqah_tip_amount, 375);
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test("SSE subscription URL builder and Relay GraphQL connection row extractor work cleanly", () => {
  const sseUrl = buildTableSseUrl(
    "https://qirim.node.muabase.org/",
    "staged-tx",
    "remittanceAttestationTable",
  );
  assert.equal(
    sseUrl,
    "https://qirim.node.muabase.org/events/sse?service=staged-tx&table=remittanceAttestationTable",
  );

  const relayRows = extractTableRows(
    {
      remittanceAttestations: {
        edges: [
          { node: { attestationId: 11, quorumSatisfied: true, verifiedWitnesses: ["v1", "v2"] } },
        ],
      },
    },
    "remittanceAttestations",
  );
  assert.equal(relayRows.length, 1);
  assert.equal(relayRows[0].quorumSatisfied, true);
});

test("GIVEN subdomain transport and selective-disclosure privacy WHEN querying proofs or sealing PII THEN routing and decryption verify cleanly", async () => {
  const { sealSelectiveDisclosureEnvelope, openSelectiveDisclosureEnvelope } = await import(
    "../dist/privacy.js"
  );

  const client = createMuabaseClient({
    endpoint: "https://node.muabase.org",
    transport: "subdomain",
  });
  assert.equal(client.resolveGraphqlUrl("ext-vault"), "https://ext-vault.node.muabase.org/graphql");
  assert.equal(client.resolveGraphqlUrl("waqf-treas"), "https://waqf-treas.node.muabase.org/graphql");
  assert.equal(client.resolveGraphqlUrl("muamalat"), "https://muamalat.node.muabase.org/graphql");

  const auditorSecret = crypto.getRandomValues(new Uint8Array(32));
  const wrongSecret = crypto.getRandomValues(new Uint8Array(32));
  const pii = JSON.stringify({ beneficiary: "Amina Q.", iban: "TR0012345" });

  const envelope = await sealSelectiveDisclosureEnvelope(pii, [
    { viewerId: "auditor", secret: auditorSecret },
  ]);
  assert.ok(!JSON.stringify(envelope).includes("Amina"));
  assert.equal(assertValidSha256Commitment(envelope.commitmentHex), envelope.commitmentHex);

  const opened = await openSelectiveDisclosureEnvelope(envelope, "auditor", auditorSecret);
  assert.equal(opened, pii);

  await assert.rejects(() => openSelectiveDisclosureEnvelope(envelope, "auditor", wrongSecret));
  await assert.rejects(() => openSelectiveDisclosureEnvelope(envelope, "outsider", auditorSecret));
});
