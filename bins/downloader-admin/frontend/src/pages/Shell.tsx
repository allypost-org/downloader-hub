import { Link, Outlet, useRouterState } from "@tanstack/react-router";
import { useAuthStore } from "@/stores/auth-store";
import { useLiveStream } from "@/lib/useLiveStream";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

const NAV = [
  { to: "/", label: "Dashboard" },
  { to: "/requests", label: "Requests" },
  { to: "/nodes", label: "Nodes" },
  { to: "/tokens", label: "Tokens" },
  { to: "/accounts", label: "Accounts" },
  { to: "/restrictions", label: "Restrictions" },
  { to: "/metrics", label: "Metrics" },
] as const;

export function Shell() {
  const me = useAuthStore((s) => s.me);
  const logout = useAuthStore((s) => s.logout);
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  useLiveStream();

  return (
    <div className="min-h-screen">
      <header className="border-b bg-card">
        <div className="mx-auto flex max-w-6xl items-center justify-between px-4 py-3">
          <div className="flex items-center gap-6">
            <span className="font-semibold">Downloader Hub</span>
            <nav className="flex gap-1">
              {NAV.map((item) => {
                const active =
                  item.to === "/"
                    ? pathname === "/"
                    : pathname.startsWith(item.to);
                return (
                  <Link key={item.to} to={item.to} className="contents">
                    <Button
                      variant={active ? "default" : "ghost"}
                      size="sm"
                      type="button"
                    >
                      {item.label}
                    </Button>
                  </Link>
                );
              })}
            </nav>
          </div>
          <div className="flex items-center gap-3">
            <span className="text-sm text-muted-foreground">
              {me?.name ?? "admin"}
              {me?.readonly && (
                <span className="ml-1.5 rounded bg-muted px-1.5 py-0.5 text-[10px] uppercase">
                  read-only
                </span>
              )}
            </span>
            <Button variant="outline" size="sm" onClick={() => void logout()}>
              Sign out
            </Button>
          </div>
        </div>
      </header>
      <main className={cn("mx-auto max-w-6xl px-4 py-6")}>
        <Outlet />
      </main>
    </div>
  );
}
