export const MIN_SLIPPAGE_PERCENT = 0.01;
export const MAX_SLIPPAGE_PERCENT = 50;
export const HIGH_SLIPPAGE_WARNING_PERCENT = 5;

/** Format a decimal slippage tolerance (e.g. 0.007 -> "0.7%") without IEEE-754 noise. */
export const formatSlippagePercent = (decimalSlippage: number): string => {
    if (!Number.isFinite(decimalSlippage) || decimalSlippage < 0) return "0%";
    const rounded = Number((decimalSlippage * 100).toFixed(2));
    return `${rounded}%`;
};

export interface CustomSlippageParseResult {
    valid: boolean;
    decimal?: number;
    error?: string;
    warning?: string;
}

/** Validate and parse a user-entered custom slippage percentage string (e.g. "0.5"). */
export const parseCustomSlippagePercent = (
    raw: string,
): CustomSlippageParseResult => {
    const trimmed = raw.trim();
    if (trimmed === "") {
        return { valid: false };
    }
    if (!/^\d*\.?\d{0,2}$/.test(trimmed)) {
        return { valid: false, error: "Enter a number with up to 2 decimals" };
    }
    const percent = Number(trimmed);
    if (!Number.isFinite(percent) || percent < MIN_SLIPPAGE_PERCENT) {
        return {
            valid: false,
            error: `Slippage must be at least ${MIN_SLIPPAGE_PERCENT}%`,
        };
    }
    if (percent > MAX_SLIPPAGE_PERCENT) {
        return {
            valid: false,
            error: `Slippage cannot exceed ${MAX_SLIPPAGE_PERCENT}%`,
        };
    }
    return {
        valid: true,
        decimal: Number((percent / 100).toFixed(4)),
        warning:
            percent > HIGH_SLIPPAGE_WARNING_PERCENT
                ? "High slippage increases front-running and price-impact risk."
                : undefined,
    };
};

export const canSubmitSwap = ({
    fromAmountNumber,
    isFromOverMaxBalance,
    fromId,
    toId,
    hasQuote,
    hasQuoteError,
}: {
    fromAmountNumber: number | undefined;
    isFromOverMaxBalance: boolean;
    fromId: number | undefined;
    toId: number | undefined;
    hasQuote: boolean;
    hasQuoteError: boolean;
}): boolean => {
    if (fromAmountNumber === undefined || !Number.isFinite(fromAmountNumber)) {
        return false;
    }
    if (fromAmountNumber <= 0) return false;
    if (isFromOverMaxBalance) return false;
    if (!fromId || !toId || fromId === toId) return false;
    if (!hasQuote || hasQuoteError) return false;
    return true;
};
