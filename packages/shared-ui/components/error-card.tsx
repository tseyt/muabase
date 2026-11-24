import { TriangleAlert } from "lucide-react";

import { Button } from "@shared/shadcn/ui/button";
import {
    Card,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@shared/shadcn/ui/card";

interface Props {
    title?: string;
    error?: Error;
    retry?: () => void;
    retryLabel?: string;
}

export const ErrorCard = ({
    error,
    retry,
    retryLabel = "Retry",
    title = "Something went wrong",
}: Props) => {
    const message =
        error?.message || "Something went wrong. Please try again.";
    return (
        <Card role="alert" className="mx-auto mt-4 w-full max-w-[350px]">
            <CardHeader>
                <div className="mx-auto">
                    <TriangleAlert
                        aria-hidden="true"
                        className="text-destructive h-12 w-12"
                    />
                </div>
                <CardTitle>{title}</CardTitle>
                <CardDescription>{message}</CardDescription>
            </CardHeader>
            {retry && (
                <CardFooter className="flex justify-end">
                    <Button
                        type="button"
                        onClick={() => {
                            retry();
                        }}
                    >
                        {retryLabel}
                    </Button>
                </CardFooter>
            )}
        </Card>
    );
};
