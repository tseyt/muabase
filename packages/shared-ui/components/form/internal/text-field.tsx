import { type ReactNode, useId } from "react";

import {
    TextInput,
    type TextInputProps,
} from "@shared/components/form/internal/text-input";
import { cn } from "@shared/lib/utils";
import { Label } from "@shared/shadcn/ui/label";

import { useFieldContext } from "../app-form";
import { FieldErrors } from "./field-errors";

type TextFieldProps = Omit<
    TextInputProps,
    "value" | "name" | "onChange" | "onBlur" | "aria-invalid" | "ariaInvalid"
> & {
    label?: string;
    rightLabel?: ReactNode;
    description?: string;
    startContent?: React.ReactNode;
    endContent?: React.ReactNode;
};

export const TextField = ({
    label,
    description,
    rightLabel,
    startContent,
    endContent,
    id,
    ...props
}: TextFieldProps) => {
    const field = useFieldContext<string>();
    const generatedId = useId();
    const inputId = id ?? generatedId;
    const descriptionId = `${inputId}-description`;
    const errorId = `${inputId}-error`;

    const isError = field.state.meta.errors.length > 0;
    const isTouched = field.state.meta.isTouched;
    const showError = isError && isTouched;

    const describedBy = [
        description ? descriptionId : undefined,
        showError ? errorId : undefined,
        props["aria-describedby"],
    ]
        .filter(Boolean)
        .join(" ");

    return (
        <div className="flex flex-col gap-2">
            {(label || rightLabel) && (
                <div className="flex items-center justify-between">
                    {label ? (
                        <Label
                            htmlFor={inputId}
                            className={cn(showError && "text-destructive")}
                        >
                            {label}
                        </Label>
                    ) : (
                        <span />
                    )}
                    {rightLabel && rightLabel}
                </div>
            )}
            <TextInput
                id={inputId}
                startContent={startContent}
                endContent={endContent}
                value={field.state.value}
                name={field.name}
                onChange={(e) => {
                    field.handleChange(e.target.value);
                }}
                onBlur={field.handleBlur}
                aria-invalid={showError}
                aria-describedby={describedBy || undefined}
                {...props}
            />
            {description && (
                <p
                    id={descriptionId}
                    className="text-muted-foreground text-sm"
                >
                    {description}
                </p>
            )}
            <FieldErrors id={errorId} meta={field.state.meta} />
        </div>
    );
};
