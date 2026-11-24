import test from "node:test";
import assert from "node:assert/strict";

import {
    canSubmitSwap,
    formatSlippagePercent,
    parseCustomSlippagePercent,
} from "./slippage.ts";

test("GIVEN decimal slippage values prone to float drift WHEN formatted THEN clean percentages are returned", () => {
    assert.equal(formatSlippagePercent(0.007), "0.7%");
    assert.equal(formatSlippagePercent(0.01), "1%");
    assert.equal(formatSlippagePercent(0.0014), "0.14%");
    assert.equal(formatSlippagePercent(-0.1), "0%");
});

test("GIVEN custom slippage input WHEN empty, out-of-range, or high THEN invalid values are blocked and high slippage warns", () => {
    assert.equal(parseCustomSlippagePercent("").valid, false);
    assert.equal(parseCustomSlippagePercent("0").valid, false);
    assert.equal(parseCustomSlippagePercent("999").valid, false);

    const safe = parseCustomSlippagePercent("0.5");
    assert.equal(safe.valid, true);
    assert.equal(safe.decimal, 0.005);
    assert.equal(safe.warning, undefined);

    const risky = parseCustomSlippagePercent("12");
    assert.equal(risky.valid, true);
    assert.equal(risky.decimal, 0.12);
    assert.match(risky.warning ?? "", /High slippage/);
});

test("GIVEN swap state WHEN amount is zero, tokens match, or quote errored THEN submission is blocked", () => {
    const valid = {
        fromAmountNumber: 10,
        isFromOverMaxBalance: false,
        fromId: 1,
        toId: 2,
        hasQuote: true,
        hasQuoteError: false,
    };
    assert.equal(canSubmitSwap(valid), true);
    assert.equal(canSubmitSwap({ ...valid, fromAmountNumber: 0 }), false);
    assert.equal(canSubmitSwap({ ...valid, fromAmountNumber: -5 }), false);
    assert.equal(canSubmitSwap({ ...valid, toId: 1 }), false);
    assert.equal(canSubmitSwap({ ...valid, hasQuote: false }), false);
    assert.equal(canSubmitSwap({ ...valid, hasQuoteError: true }), false);
    assert.equal(canSubmitSwap({ ...valid, isFromOverMaxBalance: true }), false);
});
