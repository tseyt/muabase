import * as React from "react";

import { cn } from "@shared/lib/utils";
import { Label } from "@shared/shadcn/ui/label";
import { Textarea } from "@shared/shadcn/ui/textarea";

import { useFieldContext } from "../app-form";
import { FieldErrors } from "./field-errors";

export type TextInputProps = React.InputHTMLAttributes<HTMLInputElement> & {
    ref?: React.Ref<HTMLInputElement>;
    rows?: number;
    label?: string;
};

export const TextArea = ({ label, ...props }: TextInputProps) => {
    const field = useFieldContext<string>();
    const generatedId = React.useId();
    const inputId = props.id ?? generatedId;
    const errorId = `${inputId}-error`;

    const isError = field.state.meta.errors.length > 0;
    const isTouched = field.state.meta.isTouched;
    const showError = isError && isTouched;

    return (
        <div className="flex flex-col gap-2">
            {label && (
                <div className="flex items-center justify-between">
                    <Label
                        htmlFor={inputId}
                        className={cn(showError && "text-destructive")}
                    >
                        {label}
                    </Label>
                </div>
            )}
            <Textarea
                id={inputId}
                placeholder={props.placeholder}
                value={field.state.value}
                onChange={(e) => field.handleChange(e.target.value)}
                onBlur={field.handleBlur}
                aria-invalid={showError}
                aria-describedby={showError ? errorId : undefined}
                rows={props.rows}
            />
            <FieldErrors id={errorId} meta={field.state.meta} />
        </div>
    );
};
