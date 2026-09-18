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
          <CardTitle>Active sessions (central)</CardTitle>
        </CardHeader>
        <CardContent>
          {sessions.isError ? (
            <p className="text-muted-foreground">
              Central not connected.
            </p>
          ) : sessions.data && sessions.data.length > 0 ? (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Authed</TableHead>
                  <TableHead>Role</TableHead>
                  <TableHead>Version</TableHead>
                  <TableHead>Since</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {sessions.data.map((s, i) => (
                  <TableRow key={`${s.authedId}-${s.connectedAt}-${i}`}>
                    <TableCell className="text-xs">
                      {authedLabel(s.authedId)}
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline">{s.role}</Badge>
                    </TableCell>
                    <TableCell className="text-xs">
                      {s.version ?? "\u2014"}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {new Date(Number(s.connectedAt)).toLocaleString()}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
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
  );
}
