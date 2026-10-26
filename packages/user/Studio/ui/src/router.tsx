import { createBrowserRouter } from "react-router-dom";

import { StudioLayout } from "./components/studio-layout";
import { ActionsPlaygroundPage } from "./pages/actions-playground-page";
import { InvariantsStudioPage } from "./pages/invariants-studio-page";
import { OverviewPage } from "./pages/overview-page";
import { SdkGeneratorPage } from "./pages/sdk-generator-page";
import { TableExplorerPage } from "./pages/table-explorer-page";

export const router = createBrowserRouter([
    {
        path: "/",
        element: <StudioLayout />,
        children: [
            {
                path: "/",
                element: <OverviewPage />,
            },
            {
                path: "/tables",
                element: <TableExplorerPage />,
            },
            {
                path: "/actions",
                element: <ActionsPlaygroundPage />,
            },
            {
                path: "/invariants",
                element: <InvariantsStudioPage />,
            },
            {
                path: "/sdk",
                element: <SdkGeneratorPage />,
            },
        ],
    },
]);
