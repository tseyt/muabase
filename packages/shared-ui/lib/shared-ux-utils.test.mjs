import test from "node:test";
import assert from "node:assert/strict";

import {
    b64ToPem,
    normalizeKeyToB64,
    pemToB64,
    validateB64,
} from "./b64-key-utils.ts";
import { parseError } from "./parse-error-message.ts";

const SAMPLE_B64 = Buffer.from("muabase-test-private-key-bytes").toString(
    "base64",
);

test("GIVEN a downloaded PEM private key WHEN normalized and validated THEN it passes confirmation cleanly", () => {
    const pem = b64ToPem(SAMPLE_B64);
    const normalized = normalizeKeyToB64(`  \n${pem}\r\n  `);
    assert.equal(normalized, SAMPLE_B64);
    assert.equal(validateB64(normalized), true);
    assert.equal(pemToB64(pem.replace(/\n/g, "\r\n")), SAMPLE_B64);
});

test("GIVEN an empty, whitespace-only, or malformed key WHEN validated THEN it is rejected", () => {
    assert.equal(validateB64(""), false);
    assert.equal(validateB64(normalizeKeyToB64("   \n\t ")), false);
    assert.equal(validateB64("not!valid@base64"), false);
    assert.equal(validateB64("===="), false);
});

test("GIVEN undefined, Error, string, or circular error objects WHEN parsed THEN parseError always returns a non-empty string without throwing", () => {
    assert.equal(parseError("Custom failure"), "Custom failure");
    assert.equal(parseError(new Error("Boom")), "Boom");
    assert.equal(parseError(undefined), "An unexpected error occurred");
    assert.equal(parseError(null), "An unexpected error occurred");

    const circular = {};
    circular.self = circular;
    const msg = parseError(circular);
    assert.equal(typeof msg, "string");
    assert.ok(msg.length > 0);
});
