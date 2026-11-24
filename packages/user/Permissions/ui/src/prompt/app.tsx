import { useQuery } from "@tanstack/react-query";

import { prompt } from "@psibase/common-lib";

import { ErrorCard } from "@shared/components/error-card";
import { supervisor } from "@shared/lib/supervisor";

import { LoadingSkeleton } from "./components/loading-skeleton";
import { PermissionPrompt } from "./components/permission-prompt";
import { zPermissionRequest } from "./types";

export const App = () => {
    const { data, isPending, error } = useQuery({
        queryKey: ["perms", "getContext"],
        queryFn: async () => {
            const result = await supervisor.functionCall({
                service: "perms",
                intf: "admin",
                method: "getContext",
                params: [],
            });
            return zPermissionRequest.parse(result);
        },
    });

    return (
        <div className="flex min-h-screen items-center justify-center p-6">
            {error ? (
                <ErrorCard
                    error={
                        error instanceof Error ? error : new Error(String(error))
                    }
                    title="Unable to load permission request"
                    retry={() => prompt.finished()}
                    retryLabel="Close"
                />
            ) : isPending || !data ? (
                <LoadingSkeleton />
            ) : (
                <PermissionPrompt permissionRequest={data} />
            )}
        </div>
    );
};
