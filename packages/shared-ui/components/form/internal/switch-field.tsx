import { useId } from "react";

import { cn } from "@shared/lib/utils";
import { Label } from "@shared/shadcn/ui/label";
import { Switch } from "@shared/shadcn/ui/switch";

import { useFieldContext } from "../app-form";
import { FieldErrors } from "./field-errors";

export const SwitchField = ({
    label,
    description,
    required,
    disabled,
}: {
    label: string;
    description?: string;
    required?: boolean;
    disabled?: boolean;
}) => {
    const { state, handleChange } = useFieldContext<boolean>();
    const inputId = useId();
    const descriptionId = `${inputId}-description`;
    const errorId = `${inputId}-error`;

    const isError = state.meta.errors.length > 0;
    const showError = isError && state.meta.isTouched;

    const describedBy = [
        description ? descriptionId : undefined,
        showError ? errorId : undefined,
    ]
        .filter(Boolean)
        .join(" ");

    return (
        <div>
            <div className="flex w-full justify-between">
                <Label
                    htmlFor={inputId}
                    className={cn(showError && "text-destructive")}
                >
                    {label}
                </Label>
                <Switch
                    id={inputId}
                    disabled={disabled}
                    required={required}
                    checked={state.value}
                    aria-invalid={showError}
                    aria-describedby={describedBy || undefined}
                    onCheckedChange={(e) => {
                        handleChange(e);
                    }}
                />
            </div>
            {description && (
                <p
                    id={descriptionId}
                    className="text-muted-foreground text-sm"
                >
                    {description}
                </p>
            )}
            <FieldErrors id={errorId} meta={state.meta} />
        </div>
    );
};
