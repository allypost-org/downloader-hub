import { useEffect, useState } from "react";
import { RouterProvider } from "@tanstack/react-router";
import { router } from "@/router";
import { useAuthStore } from "@/stores/auth-store";

export function App() {
  const fetchMe = useAuthStore((s) => s.fetchMe);
  const [booted, setBooted] = useState(false);

  useEffect(() => {
    void fetchMe().finally(() => setBooted(true));
  }, [fetchMe]);

  if (!booted) {
    return (
      <div className="flex min-h-screen items-center justify-center text-muted-foreground">
        Loading…
      </div>
    );
  }

  return <RouterProvider router={router} />;
}
