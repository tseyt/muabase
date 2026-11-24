export const b64ToPem = (b64: string) => {
    const header = "-----BEGIN PRIVATE KEY-----";
    const footer = "-----END PRIVATE KEY-----";
    const lines = b64.match(/.{1,64}/g) || [];
    return [header, ...lines, footer].join("\n");
};

export const pemToB64 = (pem: string) => {
    return pem
        .replace(/-----BEGIN [A-Z0-9 ]+-----/g, "")
        .replace(/-----END [A-Z0-9 ]+-----/g, "")
        .replace(/\s+/g, "");
};

export const normalizeKeyToB64 = (input: string) => pemToB64(input.trim());

export const validateB64 = (b64: string) => {
    if (b64.length === 0) return false;
    if (!/^[A-Za-z0-9+/]+={0,2}$/.test(b64)) return false;
    try {
        return atob(b64).length > 0;
    } catch {
        return false;
    }
};
