import { useMemo, useState } from "react";
import { useNavigate } from "@tanstack/react-router";
import { type ColumnDef } from "@tanstack/react-table";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  api,
  type AccountPlaceInfo,
  type AccountRef,
  type AccountUserInfo,
  type RestrictionInfo,
} from "@/lib/api";
import { useAuthStore } from "@/stores/auth-store";
import {
  appliesToAccount,
  emptyRuleForm,
  buildRule,
  refKey,
  restrictionDetail,
  ruleFormFromRestriction,
  type RuleFormState,
} from "@/lib/restrictions";
import { AccountAutocomplete } from "@/components/AccountAutocomplete";
import { AccountRequestsList } from "@/components/AccountRequestsList";
import { RestrictionRuleFields } from "@/components/RestrictionRuleFields";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { DataTable } from "@/components/DataTable";

type Tab = "users" | "places";

type RequestsTarget = {
  kind: "user" | "place";
  platform: AccountUserInfo["platform"];
  platformId: string;
  label: string;
};

function userDisplayLabel(u: AccountUserInfo): string {
  return u.displayName ?? u.username ?? u.platformId;
}

function placeDisplayLabel(p: AccountPlaceInfo): string {
  return p.name ?? p.username ?? p.platformId;
}

function requestsFilterHref(target: RequestsTarget): string {
  const params = new URLSearchParams({
    filterKind: target.kind,
    filterPlatform: target.platform,
    filterId: target.platformId,
  });
  return `/requests?${params.toString()}`;
}

function formatTime(ms: string | number): string {
  const n = typeof ms === "string" ? Number(ms) : ms;
  if (!Number.isFinite(n) || n <= 0) return "—";
  return new Date(n).toLocaleString();
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
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
    </label>
  );
}

function AccountUserFields({
  user,
  onSaved,
}: {
  user: AccountUserInfo;
  onSaved: () => void;
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
      onSaved();
    },
  });

  return (
    <div className="space-y-3">
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
        {mutation.isPending ? "Saving…" : "Save account"}
      </Button>
      <p className="text-xs text-muted-foreground">
        Leave a field blank to clear it. Only changed fields are sent.
      </p>
    </div>
  );
}

function AccountPlaceFields({
  place,
  onSaved,
}: {
  place: AccountPlaceInfo;
  onSaved: () => void;
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
      onSaved();
    },
  });

  return (
    <div className="space-y-3">
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
        {mutation.isPending ? "Saving…" : "Save account"}
      </Button>
      <p className="text-xs text-muted-foreground">
        Leave a field blank to clear it. Only changed fields are sent.
      </p>
    </div>
  );
}

/**
 * Restrictions that apply to a specific account (user or place), shown and
 * editable inside the account modal. An "add restriction" form is auto-scoped
 * to the account — no user/place picker needed.
 */
function AccountRestrictions({
  kind,
  ref_,
  users,
  places,
}: {
  kind: "user" | "place";
  ref_: AccountRef;
  users?: AccountUserInfo[];
  places?: AccountPlaceInfo[];
}) {
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const [editingId, setEditingId] = useState<string | null>(null);

  const bans = useQuery({
    queryKey: ["restrictions", "ban"],
    queryFn: () => api.listRestrictions("ban"),
  });
  const limits = useQuery({
    queryKey: ["restrictions", "limit"],
    queryFn: () => api.listRestrictions("limit"),
  });

  const applicable = useMemo(() => {
    const all = [...(bans.data ?? []), ...(limits.data ?? [])];
    return all
      .filter((r) => appliesToAccount(r, kind, ref_))
      .sort((a, b) => (a.rule.Type === b.rule.Type ? 0 : a.rule.Type === "ban" ? -1 : 1));
  }, [bans.data, limits.data, kind, ref_]);

  const remove = useMutation({
    mutationFn: (id: string) => api.removeRestriction(id),
    onSuccess: () => void qc.invalidateQueries({ queryKey: ["restrictions"] }),
  });

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium">Restrictions</span>
        <Badge variant="secondary">{applicable.length}</Badge>
      </div>
      <p className="text-xs text-muted-foreground">
        All rules that apply to this {kind}: direct rules plus any{" "}
        <code>{kind === "user" ? "any user" : "any place"}</code> wildcards.
      </p>

      {applicable.length === 0 ? (
        <p className="text-sm text-muted-foreground">No restrictions apply.</p>
      ) : (
        <ul className="space-y-1.5">
          {applicable.map((r) => {
            const direct =
              (kind === "user" && r.user != null) ||
              (kind === "place" && r.place != null);
            const other = kind === "user" ? r.place : r.user;
            return (
              <li
                key={r.id}
                className="flex items-center justify-between gap-2 rounded-md border px-2 py-1.5"
              >
                <div className="min-w-0">
                  <Badge
                    variant={r.rule.Type === "ban" ? "destructive" : "secondary"}
                    className="mr-1.5"
                  >
                    {r.rule.Type}
                  </Badge>
                  <span className="text-sm">{restrictionDetail(r.rule)}</span>
                  {!direct && (
                    <span className="ml-1.5 text-xs text-muted-foreground">
                      (wildcard{other ? ` · also scoped to ${kind === "user" ? "place" : "user"} ${refKey(other)}` : ""})
                    </span>
                  )}
                </div>
                <div className="flex shrink-0 gap-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={readonly}
                    onClick={() =>
                      setEditingId(editingId === r.id ? null : r.id)
                    }
                  >
                    {editingId === r.id ? "Cancel" : "Edit"}
                  </Button>
                  <Button
                    size="sm"
                    variant="destructive"
                    disabled={readonly || remove.isPending}
                    onClick={() => {
                      if (window.confirm("Delete this restriction?")) {
                        remove.mutate(r.id);
                      }
                    }}
                  >
                    Delete
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      {editingId &&
        applicable.some((r) => r.id === editingId) &&
        (() => {
          const row = applicable.find((r) => r.id === editingId)!;
          return (
            <EditRestrictionInline
              key={row.id}
              row={row}
              onDone={() => setEditingId(null)}
            />
          );
        })()}

      <AddRestrictionScoped kind={kind} ref_={ref_} users={users} places={places} />
    </div>
  );
}

function EditRestrictionInline({
  row,
  onDone,
}: {
  row: RestrictionInfo;
  onDone: () => void;
}) {
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const [f, setF] = useState<RuleFormState>(() =>
    ruleFormFromRestriction(row),
  );
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<RuleFormState>) =>
    setF((prev) => ({ ...prev, ...patch }));

  const save = useMutation({
    mutationFn: () => {
      const { rule, error: err } = buildRule(f);
      if (!rule) throw new Error(err);
      const body = {
        user: row.user ?? undefined,
        place: row.place ?? undefined,
        rule,
      };
      return api.updateRestriction(row.id, body);
    },
    onMutate: () => setError(null),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["restrictions"] });
      onDone();
    },
    onError: (e: Error) => setError(e.message),
  });

  return (
    <div className="space-y-3 rounded-md border p-3">
      <span className="text-xs font-medium text-muted-foreground">
        Edit rule
      </span>
      <RestrictionRuleFields f={f} set={set} readonly={readonly} />
      {error && <p className="text-xs text-destructive">{error}</p>}
      <Button
        size="sm"
        disabled={readonly || save.isPending}
        onClick={() => save.mutate()}
      >
        {save.isPending ? "Saving…" : "Save rule"}
      </Button>
    </div>
  );
}

function AddRestrictionScoped({
  kind,
  ref_,
  users,
  places,
}: {
  kind: "user" | "place";
  ref_: AccountRef;
  users?: AccountUserInfo[];
  places?: AccountPlaceInfo[];
}) {
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const [open, setOpen] = useState(false);
  const [f, setF] = useState<RuleFormState>(() => emptyRuleForm());
  const otherKind = kind === "user" ? "place" : "user";
  const [otherPlatform, setOtherPlatform] = useState<
    "telegram" | "discord"
  >(ref_.platform);
  const [otherId, setOtherId] = useState("");
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<RuleFormState>) =>
    setF((prev) => ({ ...prev, ...patch }));

  const create = useMutation({
    mutationFn: () => {
      const { rule, error: err } = buildRule(f);
      if (!rule) throw new Error(err);
      const other = otherId.trim()
        ? { platform: otherPlatform, id: otherId.trim() }
        : undefined;
      const user = kind === "user" ? ref_ : other;
      const place = kind === "place" ? ref_ : other;
      return api.createRestriction({ user, place, rule });
    },
    onMutate: () => setError(null),
    onSuccess: () => {
      setF(emptyRuleForm(f.ruleType));
      setOtherId("");
      void qc.invalidateQueries({ queryKey: ["restrictions"] });
    },
    onError: (e: Error) => setError(e.message),
  });

  return (
    <div className="space-y-3 rounded-md border border-dashed p-3">
      {!open ? (
        <Button
          size="sm"
          variant="outline"
          disabled={readonly}
          onClick={() => setOpen(true)}
        >
          Add restriction
        </Button>
      ) : (
        <>
          <div className="flex items-center justify-between">
            <span className="text-xs font-medium text-muted-foreground">
              New restriction on this {kind}
            </span>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setOpen(false);
                setError(null);
              }}
            >
              Cancel
            </Button>
          </div>
          <label className="block space-y-1">
            <span className="text-xs text-muted-foreground">
              Also scope to a {otherKind} (optional)
            </span>
            <div className="flex gap-1">
              <select
                value={otherPlatform}
                onChange={(e) =>
                  setOtherPlatform(e.target.value as "telegram" | "discord")
                }
                className="h-9 rounded-md border border-input bg-background px-2 text-sm"
              >
                <option value="telegram">telegram</option>
                <option value="discord">discord</option>
              </select>
              <div className="w-48">
                <AccountAutocomplete
                  platform={otherPlatform}
                  users={otherKind === "user" ? users : undefined}
                  places={otherKind === "place" ? places : undefined}
                  kind={otherKind}
                  value={otherId}
                  onChange={setOtherId}
                />
              </div>
            </div>
          </label>
          <RestrictionRuleFields f={f} set={set} readonly={readonly} />
          {error && <p className="text-xs text-destructive">{error}</p>}
          <Button
            size="sm"
            disabled={readonly || create.isPending}
            onClick={() => create.mutate()}
          >
            {create.isPending ? "Creating…" : "Add"}
          </Button>
        </>
      )}
    </div>
  );
}

export function AccountsPage() {
  const [tab, setTab] = useState<Tab>("users");
  const [selectedUser, setSelectedUser] = useState<AccountUserInfo | null>(null);
  const [selectedPlace, setSelectedPlace] = useState<AccountPlaceInfo | null>(
    null,
  );
  const [requestsTarget, setRequestsTarget] = useState<RequestsTarget | null>(
    null,
  );
  const qc = useQueryClient();
  const navigate = useNavigate();
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
        <div className="flex justify-end gap-1 text-right">
          <Button
            size="sm"
            variant="outline"
            onClick={() =>
              setRequestsTarget({
                kind: "user",
                platform: row.original.platform,
                platformId: row.original.platformId,
                label: userDisplayLabel(row.original),
              })
            }
          >
            Requests
          </Button>
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
        <div className="flex justify-end gap-1 text-right">
          <Button
            size="sm"
            variant="outline"
            onClick={() =>
              setRequestsTarget({
                kind: "place",
                platform: row.original.platform,
                platformId: row.original.platformId,
                label: placeDisplayLabel(row.original),
              })
            }
          >
            Requests
          </Button>
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
            “Edit” to correct a row manually and manage its restrictions.
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
        </CardContent>
      </Card>

      <Dialog
        open={selectedUser != null}
        onOpenChange={(o) => !o && setSelectedUser(null)}
      >
        <DialogContent className="max-h-[85vh] max-w-2xl overflow-y-auto">
          {selectedUser && (
            <>
              <DialogHeader>
                <DialogTitle>Edit user</DialogTitle>
                <DialogDescription>
                  Update account metadata and manage restrictions that apply to
                  this user.
                </DialogDescription>
              </DialogHeader>
              <div className="grid gap-6 md:grid-cols-2">
                <AccountUserFields
                  user={selectedUser}
                  onSaved={() => setSelectedUser(null)}
                />
                <AccountRestrictions
                  kind="user"
                  ref_={{
                    platform: selectedUser.platform,
                    id: selectedUser.platformId,
                  }}
                  users={users.data}
                  places={places.data}
                />
              </div>
            </>
          )}
        </DialogContent>
      </Dialog>

      <Dialog
        open={selectedPlace != null}
        onOpenChange={(o) => !o && setSelectedPlace(null)}
      >
        <DialogContent className="max-h-[85vh] max-w-2xl overflow-y-auto">
          {selectedPlace && (
            <>
              <DialogHeader>
                <DialogTitle>Edit place</DialogTitle>
                <DialogDescription>
                  Update account metadata and manage restrictions that apply to
                  this place.
                </DialogDescription>
              </DialogHeader>
              <div className="grid gap-6 md:grid-cols-2">
                <AccountPlaceFields
                  place={selectedPlace}
                  onSaved={() => setSelectedPlace(null)}
                />
                <AccountRestrictions
                  kind="place"
                  ref_={{
                    platform: selectedPlace.platform,
                    id: selectedPlace.platformId,
                  }}
                  users={users.data}
                  places={places.data}
                />
              </div>
            </>
          )}
        </DialogContent>
      </Dialog>

      <Dialog
        open={requestsTarget != null}
        onOpenChange={(o) => !o && setRequestsTarget(null)}
      >
        <DialogContent className="max-h-[85vh] max-w-4xl overflow-y-auto">
          {requestsTarget && (
            <>
              <DialogHeader>
                <DialogTitle>
                  {requestsTarget.kind === "user"
                    ? `Requests by ${requestsTarget.label}`
                    : `Requests in ${requestsTarget.label}`}
                </DialogTitle>
                <DialogDescription>
                  {requestsTarget.platform}:{requestsTarget.platformId} — all
                  statuses, newest first. Legacy rows without refs are omitted.
                </DialogDescription>
              </DialogHeader>
              <AccountRequestsList
                kind={requestsTarget.kind}
                platform={requestsTarget.platform}
                accountId={requestsTarget.platformId}
              />
              <div className="flex justify-end pt-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    if (!requestsTarget) return;
                    void navigate({ to: requestsFilterHref(requestsTarget) });
                    setRequestsTarget(null);
                  }}
                >
                  Open in Requests page
                </Button>
              </div>
            </>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
