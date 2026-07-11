import { useQuery } from "@tanstack/react-query";
import { api, type RequestStatusType } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { StatusBadge } from "@/components/StatusBadge";

const COUNTS: RequestStatusType[] = ["pending", "inProgress", "delivering", "failed", "done"];

function toNumber(v: string | number): number {
  return typeof v === "number" ? v : Number(v);
}

export function DashboardPage() {
  const counts = useQuery({
    queryKey: ["request-counts"],
    queryFn: () => api.requestCounts(),
    refetchInterval: 10_000,
  });

  const recentFailed = useQuery({
    queryKey: ["requests", "failed", "dashboard"],
    queryFn: async () => (await api.listRequests("failed", { limit: 5 })).page,
    refetchInterval: 15_000,
  });

  return (
    <div className="space-y-6">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {COUNTS.map((s) => (
          <Card key={s}>
            <CardHeader>
              <CardTitle className="flex items-center justify-between text-sm">
                <span className="capitalize">{s}</span>
                <StatusBadge status={s} />
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-bold">
                {counts.data ? toNumber(counts.data[s]) : "—"}
              </div>
              <p className="text-xs text-muted-foreground">
                {counts.isLoading ? "loading…" : "requests"}
              </p>
            </CardContent>
          </Card>
        ))}
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Recent failures</CardTitle>
        </CardHeader>
        <CardContent>
          {recentFailed.isLoading ? (
            <p className="text-muted-foreground">Loading…</p>
          ) : recentFailed.data && recentFailed.data.length > 0 ? (
            <ul className="space-y-2 text-sm">
              {recentFailed.data.map((r) => (
                <li
                  key={r.requestId}
                  className="flex items-start justify-between gap-3"
                >
                  <div className="min-w-0">
                    <code className="truncate text-xs">{r.requestId}</code>
                    {r.status.reason && (
                      <p className="truncate text-muted-foreground">
                        {r.status.reason}
                      </p>
                    )}
                  </div>
                  {r.errors.length > 0 && (
                    <span className="text-xs text-muted-foreground">
                      {r.errors.length} error(s)
                    </span>
                  )}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-muted-foreground">No recent failures.</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
