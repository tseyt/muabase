import type { AnyFieldMeta } from "@tanstack/react-form";

import { parseError } from "@shared/lib/parse-error-message";

type FieldErrorsProps = {
    meta: AnyFieldMeta | undefined;
    id?: string;
};

export const FieldErrors = ({ meta, id }: FieldErrorsProps) => {
    if (!meta?.isTouched || meta.errors.length === 0) return null;

    return (
        <div id={id} role="alert" className="flex flex-col gap-1">
            {meta.errors.map(parseError).map((error, index) => (
                <p key={index} className="text-destructive text-sm">
                    {error}
                </p>
            ))}
        </div>
    );
};
