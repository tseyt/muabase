import { QueryClientProvider } from "@tanstack/react-query";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "react-router-dom";

import { ThemeProvider } from "@shared/components/theme-provider";
import { queryClient } from "@shared/lib/query-client";
import { Toaster } from "@shared/shadcn/ui/sonner";
import "@shared/styles/globals.css";

import { StudioProvider } from "./context/studio-context";
import { router } from "./router";
import "./studio.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <ThemeProvider defaultTheme="dark" storageKey="muabase-studio-theme">
        <QueryClientProvider client={queryClient}>
            <StudioProvider>
                <RouterProvider router={router} />
                <Toaster />
            </StudioProvider>
        </QueryClientProvider>
    </ThemeProvider>,
);
