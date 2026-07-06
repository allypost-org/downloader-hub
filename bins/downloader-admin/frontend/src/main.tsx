import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { NuqsAdapter } from "nuqs/adapters/react";
import { App } from "@/App";
import "@/styles.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 5_000, retry: 1 },
  },
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <NuqsAdapter>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </NuqsAdapter>
  </StrictMode>,
);
