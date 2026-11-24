import { useState } from "react";

import { Check, Copy } from "lucide-react";

import { Button } from "@shared/shadcn/ui/button";
import { toast } from "@shared/shadcn/ui/sonner";

interface CopyButtonProps {
    text: string;
    ariaLabel: string;
    onCopy?: () => void;
}

export const CopyButton = ({ text, ariaLabel, onCopy }: CopyButtonProps) => {
    const [copied, setCopied] = useState(false);

    const handleCopy = async () => {
        try {
            await navigator.clipboard.writeText(text);
            setCopied(true);
            onCopy?.();
            toast("Copied to clipboard");
            window.setTimeout(() => setCopied(false), 1500);
        } catch (err) {
            console.error("Failed to copy:", err);
            toast.error("Failed to copy to clipboard");
        }
    };

    return (
        <Button
            type="button"
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => void handleCopy()}
            aria-label={copied ? `${ariaLabel} (copied)` : ariaLabel}
        >
            {copied ? (
                <Check className="h-4 w-4 text-emerald-600" />
            ) : (
                <Copy className="h-4 w-4" />
            )}
        </Button>
    );
};
