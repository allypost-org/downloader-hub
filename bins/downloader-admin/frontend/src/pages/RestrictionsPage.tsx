import { useMemo, useState } from "react";
import { type ColumnDef } from "@tanstack/react-table";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  api,
  type AccountPlatform,
  type AccountPlaceInfo,
  type AccountUserInfo,
  type RestrictionInfo,
} from "@/lib/api";
import { useAccountNames } from "@/lib/useAccountNames";
import {
  buildRule,
  emptyRuleForm,
  restrictionDetail,
  ruleFormFromRestriction,
  type BuiltRule,
  type RuleFormState,
  type RuleType,
} from "@/lib/restrictions";
import { useAuthStore } from "@/stores/auth-store";
import { AccountAutocomplete } from "@/components/AccountAutocomplete";
import { RestrictionRuleFields } from "@/components/RestrictionRuleFields";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { DataTable } from "@/components/DataTable";

type Filter = "all" | RuleType;

interface ScopeForm {
  userPlatform: AccountPlatform;
  userId: string;
  placePlatform: AccountPlatform;
  placeId: string;
}

interface FullForm extends ScopeForm, RuleFormState {}

function emptyForm(ruleType: RuleType = "ban"): FullForm {
  return {
    ...emptyRuleForm(ruleType),
    userPlatform: "telegram",
    userId: "",
    placePlatform: "telegram",
    placeId: "",
  };
}

function formFromRow(row: RestrictionInfo): FullForm {
  const f: FullForm = {
    ...emptyForm(row.rule.Type),
    ...ruleFormFromRestriction(row),
  };
  if (row.user) {
    f.userPlatform = row.user.platform;
    f.userId = row.user.id;
  }
  if (row.place) {
    f.placePlatform = row.place.platform;
    f.placeId = row.place.id;
  }
  return f;
}

function buildBody(f: FullForm): {
  body: ReturnType<typeof makeBody> | null;
  error: string | null;
} {
  if (!f.userId.trim() && !f.placeId.trim()) {
    return { body: null, error: "At least one of user or place is required." };
  }
  const { rule, error: ruleErr } = buildRule(f);
  if (!rule) return { body: null, error: ruleErr };
  return { body: makeBody(f, rule), error: null };
}

function makeBody(f: FullForm, rule: BuiltRule) {
  return {
    user: f.userId.trim()
      ? { platform: f.userPlatform, id: f.userId.trim() }
      : undefined,
    place: f.placeId.trim()
      ? { platform: f.placePlatform, id: f.placeId.trim() }
      : undefined,
    rule,
  };
}

function ScopeFields({
  f,
  set,
  users,
  places,
  readonly,
}: {
  f: ScopeForm;
  set: (patch: Partial<ScopeForm>) => void;
  users?: AccountUserInfo[];
  places?: AccountPlaceInfo[];
  readonly: boolean;
}) {
  return (
    <div className="flex flex-wrap gap-4">
      <label className="space-y-1">
        <span className="text-xs text-muted-foreground">User (optional)</span>
        <div className="flex gap-1">
          <select
            value={f.userPlatform}
            onChange={(e) =>
              set({ userPlatform: e.target.value as AccountPlatform })
            }
            disabled={readonly}
            className="h-9 rounded-md border border-input bg-background px-2 text-sm"
          >
            <option value="telegram">telegram</option>
            <option value="discord">discord</option>
          </select>
          <div className="w-48">
            <AccountAutocomplete
              platform={f.userPlatform}
              users={users}
              kind="user"
              value={f.userId}
              onChange={(id) => set({ userId: id })}
              disabled={readonly}
            />
          </div>
        </div>
      </label>
      <label className="space-y-1">
        <span className="text-xs text-muted-foreground">Place (optional)</span>
        <div className="flex gap-1">
          <select
            value={f.placePlatform}
            onChange={(e) =>
              set({ placePlatform: e.target.value as AccountPlatform })
            }
            disabled={readonly}
            className="h-9 rounded-md border border-input bg-background px-2 text-sm"
          >
            <option value="telegram">telegram</option>
            <option value="discord">discord</option>
          </select>
          <div className="w-48">
            <AccountAutocomplete
              platform={f.placePlatform}
              places={places}
              kind="place"
              value={f.placeId}
              onChange={(id) => set({ placeId: id })}
              disabled={readonly}
            />
          </div>
        </div>
      </label>
    </div>
  );
}

function RestrictionEditor({
  row,
  users,
  places,
  onClose,
}: {
  row: RestrictionInfo;
  users?: AccountUserInfo[];
  places?: AccountPlaceInfo[];
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const [f, setF] = useState<FullForm>(() => formFromRow(row));
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<FullForm>) =>
    setF((prev) => ({ ...prev, ...patch }));

  const save = useMutation({
    mutationFn: () => {
      const { body, error: err } = buildBody(f);
      if (!body) throw new Error(err ?? "invalid input");
      return api.updateRestriction(row.id, body);
    },
    onMutate: () => setError(null),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["restrictions"] });
      onClose();
    },
    onError: (e: Error) => setError(e.message),
  });

  return (
    <Card className="lg:sticky lg:top-6">
      <CardHeader>
        <CardTitle className="flex items-center justify-between text-base">
          <span>Edit restriction</span>
          <Button size="sm" variant="ghost" onClick={onClose}>
            Close
          </Button>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <ScopeFields f={f} set={set} users={users} places={places} readonly={readonly} />
        <RestrictionRuleFields f={f} set={set} readonly={readonly} />
        {error && <p className="text-sm text-destructive">{error}</p>}
        <Button disabled={readonly || save.isPending} onClick={() => save.mutate()}>
          {save.isPending ? "Saving…" : "Save"}
        </Button>
      </CardContent>
    </Card>
  );
}

export function RestrictionsPage() {
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const names = useAccountNames();

  const bans = useQuery({
    queryKey: ["restrictions", "ban"],
    queryFn: () => api.listRestrictions("ban"),
  });
  const limits = useQuery({
    queryKey: ["restrictions", "limit"],
    queryFn: () => api.listRestrictions("limit"),
  });
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

  const [filter, setFilter] = useState<Filter>("all");
  const [selected, setSelected] = useState<RestrictionInfo | null>(null);
  const [createForm, setCreateForm] = useState<FullForm>(() => emptyForm("ban"));
  const [createError, setCreateError] = useState<string | null>(null);

  const all = useMemo(() => {
    return [...(bans.data ?? []), ...(limits.data ?? [])];
  }, [bans.data, limits.data]);

  const filtered = useMemo(() => {
    if (filter === "all") return all;
    return all.filter((r) => r.rule.Type === filter);
  }, [all, filter]);

  const create = useMutation({
    mutationFn: () => {
      const { body, error: err } = buildBody(createForm);
      if (!body) throw new Error(err ?? "invalid input");
      return api.createRestriction(body);
    },
    onMutate: () => setCreateError(null),
    onSuccess: () => {
      setCreateForm(emptyForm(createForm.ruleType));
      void qc.invalidateQueries({ queryKey: ["restrictions"] });
    },
    onError: (e: Error) => setCreateError(e.message),
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.removeRestriction(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["restrictions"] });
      setSelected(null);
    },
  });

  const columns: ColumnDef<RestrictionInfo>[] = [
    {
      accessorKey: "rule.Type",
      header: "Type",
      cell: ({ row }) => (
        <Badge variant={row.original.rule.Type === "ban" ? "destructive" : "secondary"}>
          {row.original.rule.Type}
        </Badge>
      ),
    },
    {
      accessorKey: "user",
      header: "User",
      cell: ({ row }) => (
        <span className="text-sm">
          {row.original.user
            ? names.userLabelWithFallback(row.original.user)
            : <span className="text-muted-foreground">any</span>}
        </span>
      ),
    },
    {
      accessorKey: "place",
      header: "Place",
      cell: ({ row }) => (
        <span className="text-sm">
          {row.original.place
            ? names.placeLabelWithFallback(row.original.place)
            : <span className="text-muted-foreground">any</span>}
        </span>
      ),
    },
    {
      id: "detail",
      header: "Detail",
      cell: ({ row }) => (
        <span className="text-sm">{restrictionDetail(row.original.rule)}</span>
      ),
    },
    {
      id: "actions",
      header: () => <div className="text-right">Actions</div>,
      cell: ({ row }) => (
        <div className="flex justify-end gap-1" onClick={(e) => e.stopPropagation()}>
          <Button
            size="sm"
            variant="outline"
            disabled={readonly}
            onClick={() => setSelected(row.original)}
          >
            Edit
          </Button>
          <Button
            size="sm"
            variant="destructive"
            disabled={readonly || remove.isPending}
            onClick={() => {
              if (window.confirm("Delete this restriction?")) {
                remove.mutate(row.original.id);
              }
            }}
          >
            Delete
          </Button>
        </div>
      ),
      enableSorting: false,
    },
  ];

  const setCreate = (patch: Partial<FullForm>) =>
    setCreateForm((prev) => ({ ...prev, ...patch }));

  return (
    <div className="space-y-6">
      <Card>
        <CardHeader>
          <CardTitle>Create restriction</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <ScopeFields f={createForm} set={setCreate} users={users.data} places={places.data} readonly={readonly} />
          <RestrictionRuleFields f={createForm} set={setCreate} readonly={readonly} />
          {createError && <p className="text-sm text-destructive">{createError}</p>}
          <Button
            disabled={readonly || create.isPending}
            onClick={() => create.mutate()}
          >
            {create.isPending ? "Creating…" : "Create"}
          </Button>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="flex items-center justify-between">
            <span>Restrictions</span>
            <div className="flex gap-1">
              {(["all", "ban", "limit"] as const).map((t) => (
                <Badge
                  key={t}
                  variant={filter === t ? "default" : "secondary"}
                  className="cursor-pointer"
                  onClick={() => setFilter(t)}
                >
                  {t} ({t === "all" ? all.length : t === "ban" ? (bans.data?.length ?? 0) : (limits.data?.length ?? 0)})
                </Badge>
              ))}
            </div>
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className={selected ? "grid gap-4 lg:grid-cols-[1fr_360px]" : ""}>
            <div>
              {bans.isLoading || limits.isLoading ? (
                <p className="text-muted-foreground">Loading…</p>
              ) : (
                <DataTable
                  columns={columns}
                  data={filtered}
                  emptyMessage="No restrictions."
                />
              )}
            </div>
            {selected && (
              <RestrictionEditor
                key={selected.id}
                row={selected}
                users={users.data}
                places={places.data}
                onClose={() => setSelected(null)}
              />
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
