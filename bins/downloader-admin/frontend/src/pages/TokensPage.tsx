import { useState } from "react";
import { type ColumnDef } from "@tanstack/react-table";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type AuthedFullInfo } from "@/lib/api";
import { useAuthStore } from "@/stores/auth-store";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { DataTable } from "@/components/DataTable";

const columns: ColumnDef<AuthedFullInfo>[] = [
  {
    accessorKey: "name",
    header: "Name",
    cell: ({ row }) => (
      <span className="font-medium">{row.original.name}</span>
    ),
  },
  {
    accessorKey: "for",
    header: "Role",
    cell: ({ row }) => (
      <Badge variant="secondary">
        {row.original.for}
        {row.original.readonly ? " · ro" : ""}
      </Badge>
    ),
  },
  {
    accessorKey: "expiresAt",
    header: "Expires",
    cell: ({ row }) => {
      const t = row.original.expiresAt;
      if (!t) return <span className="text-xs">never</span>;
      const expired = Number(t) < Date.now();
      return (
        <span className="text-xs text-muted-foreground">
          {expired
            ? "expired"
            : new Date(Number(t)).toLocaleString()}
        </span>
      );
    },
  },
];

export function TokensPage() {
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const list = useQuery({
    queryKey: ["authed"],
    queryFn: () => api.listAuthed(),
  });

  const [name, setName] = useState("");
  const [role, setRole] = useState<"worker" | "bot" | "admin">("worker");
  const [created, setCreated] = useState<string | null>(null);

  const create = useMutation({
    mutationFn: () =>
      api.createAuthed({ name, for: role, readonly: false }),
    onSuccess: (info) => {
      setCreated(info.token);
      setName("");
      qc.invalidateQueries({ queryKey: ["authed"] });
    },
  });
  const revoke = useMutation({
    mutationFn: (id: string) => api.revokeAuthed(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["authed"] }),
  });
  const rotate = useMutation({
    mutationFn: (id: string) => api.rotateAuthed(id),
    onSuccess: (res) => {
      setCreated(res.token);
      qc.invalidateQueries({ queryKey: ["authed"] });
    },
  });
  const remove = useMutation({
    mutationFn: (id: string) => api.removeAuthed(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["authed"] }),
  });

  function actionsCell(row: AuthedFullInfo) {
    const expired =
      row.expiresAt !== null && Number(row.expiresAt) < Date.now();
    return (
      <div className="flex justify-end gap-1">
        <Button
          size="sm"
          variant="outline"
          disabled={readonly || rotate.isPending}
          onClick={() => rotate.mutate(row.id)}
        >
          Rotate
        </Button>
        {!expired && (
          <Button
            size="sm"
            variant="outline"
            disabled={readonly || revoke.isPending}
            onClick={() => revoke.mutate(row.id)}
          >
            Revoke
          </Button>
        )}
        <Button
          size="sm"
          variant="destructive"
          disabled={readonly || remove.isPending}
          onClick={() => {
            if (window.confirm(`Delete ${row.name}?`)) remove.mutate(row.id);
          }}
        >
          Delete
        </Button>
      </div>
    );
  }

  const tableColumns: ColumnDef<AuthedFullInfo>[] = [
    ...columns,
    {
      id: "actions",
      header: () => <div className="text-right">Actions</div>,
      cell: ({ row }) => (
        <div className="text-right" onClick={(e) => e.stopPropagation()}>
          {actionsCell(row.original)}
        </div>
      ),
      enableSorting: false,
    },
  ];

  return (
    <div className="space-y-6">
      {created && (
        <Card className="border-warning">
          <CardContent className="space-y-2 p-4 text-sm">
            <p className="font-semibold">
              Token created — copy it now, it won't be shown again:
            </p>
            <code className="block break-all rounded-md bg-muted p-2 text-xs">
              {created}
            </code>
            <Button size="sm" variant="outline" onClick={() => setCreated(null)}>
              Dismiss
            </Button>
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader>
          <CardTitle>Create token</CardTitle>
        </CardHeader>
        <CardContent>
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (name.trim()) create.mutate();
            }}
          >
            <label className="space-y-1">
              <span className="text-xs text-muted-foreground">Name</span>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="worker-3"
                className="w-48"
              />
            </label>
            <label className="space-y-1">
              <span className="text-xs text-muted-foreground">Role</span>
              <select
                value={role}
                onChange={(e) =>
                  setRole(e.target.value as "worker" | "bot" | "admin")
                }
                className="h-9 rounded-md border border-input bg-background px-3 text-sm"
              >
                <option value="worker">worker</option>
                <option value="bot">bot</option>
                <option value="admin">admin</option>
              </select>
            </label>
            <Button
              type="submit"
              disabled={readonly || create.isPending || !name.trim()}
            >
              Create
            </Button>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Authed identities</CardTitle>
        </CardHeader>
        <CardContent>
          {list.isLoading ? (
            <p className="text-muted-foreground">Loading…</p>
          ) : (
            <DataTable
              columns={tableColumns}
              data={list.data ?? []}
              emptyMessage="No authed identities."
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
