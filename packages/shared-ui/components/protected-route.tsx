import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";

import { useConnectAccount } from "@shared/hooks/use-connect-account";
import { useCurrentUser } from "@shared/hooks/use-current-user";
import { parseError } from "@shared/lib/parse-error-message";
import { Button } from "@shared/shadcn/ui/button";
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@shared/shadcn/ui/dialog";
import { toast } from "@shared/shadcn/ui/sonner";

export const ProtectedRoute = ({ children }: { children: React.ReactNode }) => {
    const [showModal, setShowModal] = useState(false);

    const { data: currentUser } = useCurrentUser();
    const { mutateAsync: login, isPending } = useConnectAccount();

    const navigate = useNavigate();

    useEffect(() => {
        setShowModal(currentUser === null);
    }, [currentUser]);

    const handleLogin = async () => {
        try {
            await login();
        } catch (error) {
            toast.error(parseError(error));
        }
    };

    return (
        <Dialog
            open={showModal}
            onOpenChange={(open) => {
                const isRefusingToLogin = !open && currentUser === null;
                if (isRefusingToLogin) {
                    navigate("/");
                } else {
                    setShowModal(open);
                }
            }}
        >
            {currentUser ? children : null}
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>Login required</DialogTitle>
                    <DialogDescription>
                        You must be logged in to visit this page.
                    </DialogDescription>
                    <div className="flex w-full justify-end">
                        <Button
                            disabled={isPending}
                            onClick={() => void handleLogin()}
                        >
                            {isPending ? "Signing in..." : "Login"}
                        </Button>
                    </div>
                </DialogHeader>
            </DialogContent>
        </Dialog>
    );
};
