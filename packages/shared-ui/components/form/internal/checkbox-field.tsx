import { useId } from "react";

import { cn } from "@shared/lib/utils";
import { Checkbox } from "@shared/shadcn/ui/checkbox";
import { Label } from "@shared/shadcn/ui/label";

import { FieldErrors } from "./field-errors";
import { useFieldContext } from "../app-form";

export const CheckboxField = ({
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
            <div className="flex w-full items-center gap-2">
                <Label
                    htmlFor={inputId}
                    className={cn(
                        showError && "text-destructive",
                        "cursor-pointer",
                    )}
                >
                    {label}
                </Label>
                <Checkbox
                    id={inputId}
                    disabled={disabled}
                    required={required}
                    checked={state.value}
                    aria-invalid={showError}
                    aria-describedby={describedBy || undefined}
                    onCheckedChange={(checked) => handleChange(!!checked)}
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
