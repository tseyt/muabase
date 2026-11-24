import test from "node:test";
import assert from "node:assert/strict";

import {
    calculateApprovalRatio,
    formatApprovalPercent,
} from "./approval.ts";

test("GIVEN zero governance votes WHEN computing approval percentage THEN it returns 0% instead of NaN%", () => {
    assert.equal(calculateApprovalRatio(0, 0), 0);
    assert.equal(formatApprovalPercent(0, 0), "0%");
});

test("GIVEN mixed Yes and No votes WHEN computing approval percentage THEN it divides yes by total votes", () => {
    assert.equal(calculateApprovalRatio(1, 1), 0.5);
    assert.equal(formatApprovalPercent(1, 1), "50%");
    assert.equal(formatApprovalPercent(2, 3), "40%");
    assert.equal(formatApprovalPercent(2, 1), "66%");
    assert.equal(formatApprovalPercent(3, 0), "100%");
});
