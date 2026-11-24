/** Gets a string message from the error */
export const parseError = (error: unknown): string => {
    if (typeof error === "string") {
        return error || "An unexpected error occurred";
    }
    if (
        typeof error === "object" &&
        error !== null &&
        "message" in error &&
        typeof error.message === "string" &&
        error.message.trim().length > 0
    ) {
        return error.message;
    }
    if (error == null) {
        return "An unexpected error occurred";
    }
    console.error("Unrecognized error", error);
    try {
        return JSON.stringify(error) ?? "An unexpected error occurred";
    } catch {
        return String(error) || "An unexpected error occurred";
    }
};
