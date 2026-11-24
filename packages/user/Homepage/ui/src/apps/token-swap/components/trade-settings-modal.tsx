import { useEffect, useState } from "react";

import { cn } from "@shared/lib/utils";
import { Button } from "@shared/shadcn/ui/button";
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@shared/shadcn/ui/dialog";
import { Input } from "@shared/shadcn/ui/input";
import { Label } from "@shared/shadcn/ui/label";

import { useSlippageTolerance } from "../hooks/use-slippage-tolerance";
import { parseCustomSlippagePercent } from "../slippage";

interface TradeSettingsModalProps {
    show: boolean;
    openChange: (open: boolean) => void;
}

const PRESET_SLIPPAGES = [0.5, 1, 3] as const;

export const TradeSettingsModal = ({
    show,
    openChange,
}: TradeSettingsModalProps) => {
    const [slippageTolerance, setSlippageTolerance] = useSlippageTolerance();
    // slippageTolerance is stored as decimal (0.01 = 1%)

    const [customValue, setCustomValue] = useState<string>("");
    const [customError, setCustomError] = useState<string | undefined>();
    const [customWarning, setCustomWarning] = useState<string | undefined>();
    const [activePreset, setActivePreset] = useState<number | string>("custom");

    const displayedSlippage = Number((slippageTolerance * 100).toFixed(2));

    useEffect(() => {
        if (!show) return;

        const presetMatch = PRESET_SLIPPAGES.find(
            (p) => Math.abs(p - displayedSlippage) < 0.001,
        );

        setCustomError(undefined);
        if (presetMatch !== undefined) {
            setActivePreset(presetMatch);
            setCustomValue("");
            setCustomWarning(undefined);
        } else {
            setActivePreset("custom");
            const formatted = displayedSlippage
                .toFixed(2)
                .replace(/\.?0+$/, "");
            setCustomValue(formatted);
            setCustomWarning(parseCustomSlippagePercent(formatted).warning);
        }
    }, [show, displayedSlippage]);

    const handlePresetClick = (preset: number) => {
        const decimalValue = preset / 100;
        setSlippageTolerance(decimalValue);
        setActivePreset(preset);
        setCustomValue("");
        setCustomError(undefined);
        setCustomWarning(undefined);
    };

    const handleCustomChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        const val = e.target.value.trim();

        if (val === "") {
            setCustomValue("");
            setCustomError(undefined);
            setCustomWarning(undefined);
            return;
        }

        if (/^\d*\.?\d{0,2}$/.test(val)) {
            setCustomValue(val);
            const parsed = parseCustomSlippagePercent(val);
            setCustomError(parsed.error);
            setCustomWarning(parsed.warning);
            if (parsed.valid && parsed.decimal !== undefined) {
                setSlippageTolerance(parsed.decimal);
                setActivePreset("custom");
            }
        }
    };

    return (
        <Dialog open={show} onOpenChange={openChange}>
            <DialogContent className="sm:max-w-md">
                <DialogHeader>
                    <DialogTitle>Trade Settings</DialogTitle>
                    <DialogDescription>
                        Configure maximum slippage tolerance for swaps.
                    </DialogDescription>
                </DialogHeader>

                <div className="space-y-6 py-4">
                    <div className="space-y-3">
                        <Label htmlFor="slippage">Slippage Tolerance</Label>

                        <div className="flex gap-2">
                            {PRESET_SLIPPAGES.map((preset) => (
                                <Button
                                    key={preset}
                                    type="button"
                                    variant={
                                        activePreset === preset
                                            ? "default"
                                            : "outline"
                                    }
                                    size="sm"
                                    className={cn(
                                        "flex-1",
                                        activePreset === preset &&
                                            "bg-primary text-primary-foreground",
                                    )}
                                    onClick={() => handlePresetClick(preset)}
                                >
                                    {preset}%
                                </Button>
                            ))}
                        </div>

                        <div className="flex items-center gap-2">
                            <Input
                                id="slippage"
                                type="text"
                                inputMode="decimal"
                                value={customValue}
                                onChange={handleCustomChange}
                                placeholder="Custom"
                                aria-invalid={Boolean(customError)}
                                className="max-w-[120px]"
                            />
                            <span className="text-muted-foreground text-sm">
                                %
                            </span>
                        </div>

                        {customError && (
                            <p role="alert" className="text-destructive text-xs">
                                {customError}
                            </p>
                        )}
                        {!customError && customWarning && (
                            <p className="text-xs text-amber-600 dark:text-amber-400">
                                {customWarning}
                            </p>
                        )}

                        <p className="text-muted-foreground text-xs">
                            Your transaction may revert if price changes
                            unfavorably by more than this percentage.
                        </p>
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    );
};
