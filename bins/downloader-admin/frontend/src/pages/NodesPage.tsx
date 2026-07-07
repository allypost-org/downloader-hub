import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { useAuthedNames } from "@/lib/useAuthedNames";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

export function NodesPage() {
  const { name: authedName } = useAuthedNames();

  function authedLabel(id: string): string {
    const n = authedName(id);
    return n ? `${n} (${id.slice(-6)})` : id.slice(-12);
  }

  const connections = useQuery({
    queryKey: ["connections"],
    queryFn: () => api.connections(),
    refetchInterval: 10_000,
  });
  const sessions = useQuery({
    queryKey: ["central-sessions"],
    queryFn: () => api.centralSessions(),
    refetchInterval: 10_000,
    retry: 0,
  });
  const parked = useQuery({
    queryKey: ["central-parked"],
    queryFn: () => api.centralParkedWorkers(),
    refetchInterval: 10_000,
    retry: 0,
  });

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>Connections</CardTitle>
        </CardHeader>
        <CardContent>
          {connections.isLoading ? (
            <p className="text-muted-foreground">Loading…</p>
          ) : connections.data && connections.data.connections.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Authed</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead>Version</TableHead>
                  <TableHead>Last seen</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {connections.data.connections.map((c, i) => (
                  <TableRow key={`${c.authed}-${i}`}>
                    <TableCell className="text-xs">
                      {authedLabel(c.authed)}
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary">{c.role}</Badge>
                    </TableCell>
                    <TableCell className="text-xs">
                      {c.version ?? "—"}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {new Date(Number(c.lastSeen)).toLocaleTimeString()}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          ) : (
            <p className="text-muted-foreground">No connections.</p>
          )}
        </CardContent>
      </Card>

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Active sessions (central)</CardTitle>
          </CardHeader>
          <CardContent>
            {sessions.isError ? (
              <p className="text-sm text-muted-foreground">
                Central not connected.
              </p>
            ) : sessions.data && sessions.data.length > 0 ? (
              <ul className="space-y-1 text-sm">
                {sessions.data.map((s, i) => (
                  <li
                    key={`${s.authedId}-${s.connectedAt}-${i}`}
                    className="flex justify-between"
                  >
                    <span className="text-xs">{authedLabel(s.authedId)}</span>
                    <Badge variant="outline">{s.role}</Badge>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-muted-foreground">No active sessions.</p>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Parked workers (central)</CardTitle>
          </CardHeader>
          <CardContent>
            {parked.isError ? (
              <p className="text-sm text-muted-foreground">
                Central not connected.
              </p>
            ) : parked.data && parked.data.length > 0 ? (
              <ul className="space-y-1 text-sm">
                {parked.data.map((w, i) => (
                  <li
                    key={`${w.authedId}-${w.since}-${i}`}
                    className="flex justify-between"
                  >
                    <span className="text-xs">{authedLabel(w.authedId)}</span>
                    <span className="text-xs text-muted-foreground">
                      since {new Date(w.since).toLocaleTimeString()}
                    </span>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-muted-foreground">No parked workers.</p>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
