import { useId } from "react";

import { cn } from "@shared/lib/utils";
import { Input } from "@shared/shadcn/ui/input";
import { Label } from "@shared/shadcn/ui/label";

import { useFieldContext } from "../app-form";
import { FieldErrors } from "./field-errors";

export const NumberField = ({
    label,
    placeholder,
    description,
    required,
    disabled,
}: {
    label: string;
    placeholder?: string;
    description?: string;
    required?: boolean;
    disabled?: boolean;
}) => {
    const { state, handleBlur, handleChange, name } = useFieldContext<number>();
    const inputId = useId();
    const descriptionId = `${inputId}-description`;
    const errorId = `${inputId}-error`;

    const isError = state.meta.errors.length > 0;
    const isTouched = state.meta.isTouched;
    const showError = isError && isTouched;

    const describedBy = [
        description ? descriptionId : undefined,
        showError ? errorId : undefined,
    ]
        .filter(Boolean)
        .join(" ");

    return (
        <div className="flex flex-col gap-2">
            <Label
                htmlFor={inputId}
                className={cn(showError && "text-destructive")}
            >
                {label}
            </Label>
            <Input
                id={inputId}
                type="number"
                name={name}
                required={required}
                value={state.value}
                placeholder={placeholder}
                onChange={(e) => handleChange(e.target.valueAsNumber)}
                onFocus={(e) => e.target.select()}
                onBlur={handleBlur}
                aria-invalid={showError}
                aria-describedby={describedBy || undefined}
                className={disabled ? "hide-number-input-arrows" : undefined}
                disabled={disabled}
            />
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
