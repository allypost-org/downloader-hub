import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  api,
  type AccountPlaceInfo,
  type AccountPlatform,
  type AccountRef,
  type AccountUserInfo,
  type SecretEntry,
} from "@/lib/api";
import { useAuthStore } from "@/stores/auth-store";
import { AccountAutocomplete } from "@/components/AccountAutocomplete";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";

const PLATFORMS: AccountPlatform[] = ["telegram", "discord"];

export function SecretsPage() {
  const qc = useQueryClient();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const list = useQuery({
    queryKey: ["secrets"],
    queryFn: api.listSecrets,
  });
  const users = useQuery({ queryKey: ["account-users"], queryFn: api.listAccountUsers });
  const places = useQuery({ queryKey: ["account-places"], queryFn: api.listAccountPlaces });

  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [allowedUsers, setAllowedUsers] = useState<AccountRef[]>([]);
  const [allowedPlaces, setAllowedPlaces] = useState<AccountRef[]>([]);

  const create = useMutation({
    mutationFn: () =>
      api.setSecret({ name: name.trim(), value, allowedUsers, allowedPlaces }),
    onSuccess: () => {
      setName("");
      setValue("");
      setAllowedUsers([]);
      setAllowedPlaces([]);
      qc.invalidateQueries({ queryKey: ["secrets"] });
    },
  });

  const remove = useMutation({
    mutationFn: (secretName: string) => api.removeSecret(secretName),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["secrets"] }),
  });

  function removeSecret(secretName: string) {
    if (window.confirm(`Delete secret "${secretName}"?`)) {
      remove.mutate(secretName);
    }
  }

  function edit(secret: SecretEntry) {
    setName(secret.name);
    setValue(secret.value);
    setAllowedUsers(secret.allowedUsers ?? []);
    setAllowedPlaces(secret.allowedPlaces ?? []);
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold">Secrets</h1>
        <p className="mt-1 max-w-3xl text-sm text-muted-foreground">
          Per-platform cookies used by workers for authenticated downloads
          (e.g. age-gated Instagram posts). Named by platform; workers match by
          URL host. Optionally restrict a secret to specific users/places -
          empty lists mean anyone.
        </p>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>
            {list.data?.find((s) => s.name === name.trim()) ? "Update" : "Create"}{" "}
            secret
          </CardTitle>
        </CardHeader>
        <CardContent>
          <form
            className="space-y-3"
            onSubmit={(e) => {
              e.preventDefault();
              if (name.trim() && value) create.mutate();
            }}
          >
            <label className="block space-y-1.5">
              <span className="text-sm font-medium">Name</span>
              <Input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="instagram"
                spellCheck={false}
                disabled={readonly}
              />
            </label>
            <label className="block space-y-1.5">
              <span className="text-sm font-medium">Cookie value</span>
              <textarea
                value={value}
                onChange={(e) => setValue(e.target.value)}
                placeholder="sessionid=...; ds_user_id=..."
                spellCheck={false}
                disabled={readonly}
                className="min-h-20 w-full rounded-md border border-input bg-background px-3 py-2 font-mono text-xs shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
              />
            </label>

            <RefList
              label="Allowed users"
              kind="user"
              list={allowedUsers}
              onChange={setAllowedUsers}
              users={users.data}
              readonly={readonly}
            />
            <RefList
              label="Allowed places"
              kind="place"
              list={allowedPlaces}
              onChange={setAllowedPlaces}
              places={places.data}
              readonly={readonly}
            />

            {create.isError && (
              <p className="text-sm text-destructive">
                {create.error instanceof Error
                  ? create.error.message
                  : "Unable to save secret."}
              </p>
            )}
            <Button
              type="submit"
              disabled={readonly || create.isPending || !name.trim() || !value}
            >
              {create.isPending ? "Saving..." : "Save"}
            </Button>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Stored secrets</CardTitle>
        </CardHeader>
        <CardContent>
          {list.isLoading ? (
            <p className="text-muted-foreground">Loading&hellip;</p>
          ) : list.data && list.data.length > 0 ? (
            <div className="space-y-3">
              {list.data.map((secret) => (
                <div key={secret.name} className="space-y-1 rounded-md border p-3">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium">{secret.name}</span>
                    <div className="flex items-center gap-1">
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={readonly}
                        onClick={() => edit(secret)}
                      >
                        Edit
                      </Button>
                      <Button
                        size="sm"
                        variant="destructive"
                        disabled={readonly || remove.isPending}
                        onClick={() => removeSecret(secret.name)}
                      >
                        Delete
                      </Button>
                    </div>
                  </div>
                  <code className="block break-all rounded-md bg-muted p-2 text-xs">
                    {secret.value}
                  </code>
                  <RestrictionSummary secret={secret} />
                  <span className="text-xs text-muted-foreground">
                    Updated {new Date(Number(secret.updatedAt)).toLocaleString()}
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-muted-foreground">No secrets stored.</p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function RestrictionSummary({ secret }: { secret: SecretEntry }) {
  const users = secret.allowedUsers ?? [];
  const places = secret.allowedPlaces ?? [];
  if (users.length === 0 && places.length === 0) {
    return (
      <span className="text-xs text-muted-foreground">Allowed: anyone</span>
    );
  }
  return (
    <div className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
      <span>Allowed:</span>
      {users.map((u) => (
        <Badge key={`u:${u.platform}:${u.id}`} variant="secondary">
          user {u.platform} {u.id}
        </Badge>
      ))}
      {places.map((p) => (
        <Badge key={`p:${p.platform}:${p.id}`} variant="secondary">
          place {p.platform} {p.id}
        </Badge>
      ))}
    </div>
  );
}

function RefList({
  label,
  kind,
  list,
  onChange,
  users,
  places,
  readonly,
}: {
  label: string;
  kind: "user" | "place";
  list: AccountRef[];
  onChange: (next: AccountRef[]) => void;
  users?: AccountUserInfo[];
  places?: AccountPlaceInfo[];
  readonly: boolean;
}) {
  const [platform, setPlatform] = useState<AccountPlatform>("telegram");
  const [id, setId] = useState("");

  function add() {
    const trimmed = id.trim();
    if (!trimmed) return;
    if (list.some((r) => r.platform === platform && r.id === trimmed)) {
      setId("");
      return;
    }
    onChange([...list, { platform, id: trimmed }]);
    setId("");
  }

  return (
    <div className="space-y-1.5">
      <span className="text-sm font-medium">{label}</span>
      {list.length === 0 ? (
        <span className="text-xs text-muted-foreground">Any (no restriction)</span>
      ) : (
        <div className="flex flex-wrap gap-1">
          {list.map((r) => (
            <Badge key={`${r.platform}:${r.id}`} variant="secondary" className="gap-1">
              {r.platform} {r.id}
              <button
                type="button"
                disabled={readonly}
                onClick={() =>
                  onChange(list.filter((x) => !(x.platform === r.platform && x.id === r.id)))
                }
                className="ml-0.5 text-muted-foreground hover:text-foreground disabled:opacity-50"
                aria-label={`Remove ${r.platform} ${r.id}`}
              >
                &times;
              </button>
            </Badge>
          ))}
        </div>
      )}
      <div className="flex gap-1">
        <select
          value={platform}
          onChange={(e) => setPlatform(e.target.value as AccountPlatform)}
          disabled={readonly}
          className="h-9 rounded-md border border-input bg-background px-2 text-sm"
        >
          {PLATFORMS.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
        <div className="w-56">
          <AccountAutocomplete
            platform={platform}
            kind={kind}
            users={users}
            places={places}
            value={id}
            onChange={setId}
            disabled={readonly}
          />
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={readonly || !id.trim()}
          onClick={add}
        >
          Add
        </Button>
      </div>
    </div>
  );
}
