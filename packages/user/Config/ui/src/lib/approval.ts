/** Calculate the approval ratio in [0, 1] for staged governance responses. */
export const calculateApprovalRatio = (
    yesCount: number,
    noCount: number,
): number => {
    const safeYes = Math.max(0, yesCount);
    const safeNo = Math.max(0, noCount);
    const total = safeYes + safeNo;
    return total > 0 ? safeYes / total : 0;
};

/** Format an approval ratio as a whole-number percentage (e.g. "66%"). */
export const formatApprovalPercent = (
    yesCount: number,
    noCount: number,
): string => `${Math.floor(calculateApprovalRatio(yesCount, noCount) * 100)}%`;
