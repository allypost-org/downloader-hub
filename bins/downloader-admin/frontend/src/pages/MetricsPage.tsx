import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export function MetricsPage() {
  const metrics = useQuery({
    queryKey: ["metrics"],
    queryFn: () => api.metrics(),
    refetchInterval: 15_000,
    retry: 0,
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle>Central metrics (Prometheus)</CardTitle>
      </CardHeader>
      <CardContent>
        {metrics.isError ? (
          <p className="text-muted-foreground">Central metrics unavailable.</p>
        ) : metrics.isLoading ? (
          <p className="text-muted-foreground">Loading…</p>
        ) : (
          <pre className="max-h-[70vh] overflow-auto rounded-md bg-muted p-3 text-xs leading-relaxed">
            {metrics.data}
          </pre>
        )}
      </CardContent>
    </Card>
  );
}
