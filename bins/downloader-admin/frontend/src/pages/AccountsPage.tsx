import { useState } from "react";
import { type ColumnDef } from "@tanstack/react-table";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  api,
  type AccountPlaceInfo,
  type AccountUserInfo,
} from "@/lib/api";
import { useAuthStore } from "@/stores/auth-store";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { DataTable } from "@/components/DataTable";

type Tab = "users" | "places";

function formatTime(ms: string | number): string {
  const n = typeof ms === "string" ? Number(ms) : ms;
  if (!Number.isFinite(n) || n <= 0) return "—";
  return new Date(n).toLocaleString();
}

function AccountUserEditor({
  user,
  onClose,
}: {
  user: AccountUserInfo;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const [username, setUsername] = useState(user.username ?? "");
  const [displayName, setDisplayName] = useState(user.displayName ?? "");
  const [isBot, setIsBot] = useState(
    user.isBot === null ? "" : user.isBot ? "true" : "false",
  );

  const mutation = useMutation({
    mutationFn: () => {
      const body: Record<string, string | boolean | null> = {};
      const u = username.trim() || null;
      const d = displayName.trim() || null;
      const b = isBot === "" ? null : isBot === "true";
      if (u !== user.username) body.username = u;
      if (d !== user.displayName) body.displayName = d;
      if (b !== user.isBot) body.isBot = b;
      return api.updateAccountUser(user.id, body);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["accounts"] });
      void qc.invalidateQueries({ queryKey: ["account-names"] });
      onClose();
    },
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between text-sm">
          <span>Edit user</span>
          <Button size="sm" variant="ghost" onClick={onClose}>
            Close
          </Button>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="text-xs text-muted-foreground">
          <Badge variant="secondary">{user.platform}</Badge>{" "}
          <code>{user.platformId}</code>
        </div>
        <Field label="Username">
          <Input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder="(none)"
          />
        </Field>
        <Field label="Display name">
          <Input
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="(none)"
          />
        </Field>
        <Field label="Bot">
          <select
            className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm"
            value={isBot}
            onChange={(e) => setIsBot(e.target.value)}
          >
            <option value="">(unknown)</option>
            <option value="false">no</option>
            <option value="true">yes</option>
          </select>
        </Field>
        {mutation.isError && (
          <p className="text-xs text-destructive">
            {(mutation.error as Error)?.message ?? "update failed"}
          </p>
        )}
        <Button
          size="sm"
          disabled={readonly || mutation.isPending}
          onClick={() => mutation.mutate()}
        >
          {mutation.isPending ? "Saving…" : "Save"}
        </Button>
        <p className="text-xs text-muted-foreground">
          Leave a field blank to clear it. Only changed fields are sent.
        </p>
      </CardContent>
    </Card>
  );
}

function AccountPlaceEditor({
  place,
  onClose,
}: {
  place: AccountPlaceInfo;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const [kind, setKind] = useState(place.kind ?? "");
  const [name, setName] = useState(place.name ?? "");
  const [username, setUsername] = useState(place.username ?? "");
  const [parentPlatformId, setParentPlatformId] = useState(
    place.parentPlatformId ?? "",
  );

  const mutation = useMutation({
    mutationFn: () => {
      const body: Record<string, string | null> = {};
      const k = kind.trim() || null;
      const n = name.trim() || null;
      const u = username.trim() || null;
      const p = parentPlatformId.trim() || null;
      if (k !== place.kind) body.kind = k;
      if (n !== place.name) body.name = n;
      if (u !== place.username) body.username = u;
      if (p !== place.parentPlatformId) body.parentPlatformId = p;
      return api.updateAccountPlace(place.id, body);
    },
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["accounts"] });
      void qc.invalidateQueries({ queryKey: ["account-names"] });
      onClose();
    },
  });

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between text-sm">
          <span>Edit place</span>
          <Button size="sm" variant="ghost" onClick={onClose}>
            Close
          </Button>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="text-xs text-muted-foreground">
          <Badge variant="secondary">{place.platform}</Badge>{" "}
          <code>{place.platformId}</code>
        </div>
        <Field label="Kind">
          <Input
            value={kind}
            onChange={(e) => setKind(e.target.value)}
            placeholder="(none)"
          />
        </Field>
        <Field label="Name">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="(none)"
          />
        </Field>
        <Field label="Username">
          <Input
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            placeholder="(none)"
          />
        </Field>
        <Field label="Parent platform id">
          <Input
            value={parentPlatformId}
            onChange={(e) => setParentPlatformId(e.target.value)}
            placeholder="(none)"
          />
        </Field>
        {mutation.isError && (
          <p className="text-xs text-destructive">
            {(mutation.error as Error)?.message ?? "update failed"}
          </p>
        )}
        <Button
          size="sm"
          disabled={readonly || mutation.isPending}
          onClick={() => mutation.mutate()}
        >
          {mutation.isPending ? "Saving…" : "Save"}
        </Button>
        <p className="text-xs text-muted-foreground">
          Leave a field blank to clear it. Only changed fields are sent.
        </p>
      </CardContent>
    </Card>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block space-y-1">
      <span className="text-xs font-medium text-muted-foreground">
        {label}
      </span>
      {children}
    </label>
  );
}

export function AccountsPage() {
  const [tab, setTab] = useState<Tab>("users");
  const [selectedUser, setSelectedUser] = useState<AccountUserInfo | null>(null);
  const [selectedPlace, setSelectedPlace] = useState<AccountPlaceInfo | null>(
    null,
  );
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);

  const users = useQuery({
    queryKey: ["accounts", "users"],
    queryFn: () => api.listAccountUsers(),
    refetchInterval: 30_000,
  });
  const places = useQuery({
    queryKey: ["accounts", "places"],
    queryFn: () => api.listAccountPlaces(),
    refetchInterval: 30_000,
  });

  const backfill = useMutation({
    mutationFn: () => api.backfillOrderedRefs(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["requests"] });
    },
  });

  const userColumns: ColumnDef<AccountUserInfo>[] = [
    {
      accessorKey: "platform",
      header: "Platform",
      cell: ({ row }) => (
        <Badge variant="secondary">{row.original.platform}</Badge>
      ),
    },
    {
      accessorKey: "platformId",
      header: "ID",
      cell: ({ row }) => (
        <code className="text-xs">{row.original.platformId}</code>
      ),
    },
    {
      accessorKey: "username",
      header: "Username",
      cell: ({ row }) => (
        <span className="font-mono text-xs">
          {row.original.username ?? "—"}
        </span>
      ),
    },
    {
      accessorKey: "displayName",
      header: "Display name",
      cell: ({ row }) => row.original.displayName ?? "—",
    },
    {
      accessorKey: "isBot",
      header: "Bot",
      cell: ({ row }) =>
        row.original.isBot ? <Badge variant="warning">bot</Badge> : "—",
    },
    {
      accessorKey: "lastSeen",
      header: "Last seen",
      cell: ({ row }) => (
        <span className="text-xs text-muted-foreground">
          {formatTime(row.original.lastSeen)}
        </span>
      ),
    },
    {
      id: "actions",
      header: () => <div className="text-right">Actions</div>,
      enableSorting: false,
      cell: ({ row }) => (
        <div className="text-right">
          <Button
            size="sm"
            variant="outline"
            onClick={() => setSelectedUser(row.original)}
          >
            Edit
          </Button>
        </div>
      ),
    },
  ];

  const placeColumns: ColumnDef<AccountPlaceInfo>[] = [
    {
      accessorKey: "platform",
      header: "Platform",
      cell: ({ row }) => (
        <Badge variant="secondary">{row.original.platform}</Badge>
      ),
    },
    {
      accessorKey: "platformId",
      header: "ID",
      cell: ({ row }) => (
        <code className="text-xs">{row.original.platformId}</code>
      ),
    },
    {
      accessorKey: "kind",
      header: "Kind",
      cell: ({ row }) =>
        row.original.kind ? (
          <Badge variant="outline">{row.original.kind}</Badge>
        ) : (
          "—"
        ),
    },
    {
      accessorKey: "name",
      header: "Name",
      cell: ({ row }) => row.original.name ?? "—",
    },
    {
      accessorKey: "username",
      header: "Username",
      cell: ({ row }) => (
        <span className="font-mono text-xs">
          {row.original.username ?? "—"}
        </span>
      ),
    },
    {
      accessorKey: "parentPlatformId",
      header: "Parent",
      cell: ({ row }) => (
        <span className="font-mono text-xs">
          {row.original.parentPlatformId ?? "—"}
        </span>
      ),
    },
    {
      accessorKey: "lastSeen",
      header: "Last seen",
      cell: ({ row }) => (
        <span className="text-xs text-muted-foreground">
          {formatTime(row.original.lastSeen)}
        </span>
      ),
    },
    {
      id: "actions",
      header: () => <div className="text-right">Actions</div>,
      enableSorting: false,
      cell: ({ row }) => (
        <div className="text-right">
          <Button
            size="sm"
            variant="outline"
            onClick={() => setSelectedPlace(row.original)}
          >
            Edit
          </Button>
        </div>
      ),
    },
  ];

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <span>Accounts</span>
            <Button
              size="sm"
              variant="outline"
              disabled={readonly || backfill.isPending}
              onClick={() => {
                if (
                  window.confirm(
                    "Backfill orderedBy/orderedIn on existing requests from idempotency keys + stored metadata? Discord fully, Telegram DMs fully, Telegram groups orderedIn-only (the ordering user is unrecoverable). Runs in the background on the backend.",
                  )
                ) {
                  backfill.mutate();
                }
              }}
            >
              {backfill.isPending ? "Starting…" : "Backfill refs"}
            </Button>
          </CardTitle>
          <p className="text-sm text-muted-foreground">
            End-users and chats/servers seen by the bots. Refreshed by bots on
            each message — metadata for inactive entities may be stale. Use
            “Edit” to correct a row manually.
          </p>
        </CardHeader>
        <CardContent>
          <div className="mb-4 flex gap-2">
            <Badge
              variant={tab === "users" ? "default" : "secondary"}
              className="cursor-pointer"
              onClick={() => setTab("users")}
            >
              Users ({users.data?.length ?? 0})
            </Badge>
            <Badge
              variant={tab === "places" ? "default" : "secondary"}
              className="cursor-pointer"
              onClick={() => setTab("places")}
            >
              Places ({places.data?.length ?? 0})
            </Badge>
          </div>

          <div
            className={
              selectedUser || selectedPlace
                ? "grid gap-4 lg:grid-cols-[1fr_360px]"
                : ""
            }
          >
            <div>
              {tab === "users" ? (
                <DataTable
                  columns={userColumns}
                  data={users.data ?? []}
                  emptyMessage="No users recorded yet."
                />
              ) : (
                <DataTable
                  columns={placeColumns}
                  data={places.data ?? []}
                  emptyMessage="No places recorded yet."
                />
              )}
            </div>

            {tab === "users" && selectedUser && (
              <AccountUserEditor
                user={selectedUser}
                onClose={() => setSelectedUser(null)}
              />
            )}
            {tab === "places" && selectedPlace && (
              <AccountPlaceEditor
                place={selectedPlace}
                onClose={() => setSelectedPlace(null)}
              />
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
