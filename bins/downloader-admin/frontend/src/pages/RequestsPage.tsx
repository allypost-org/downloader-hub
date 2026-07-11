import { useMemo, useState } from "react";
import {
  useMutation,
  useInfiniteQuery,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { parseAsString, parseAsStringLiteral, useQueryStates } from "nuqs";
import {
  api,
  type AccountPlatform,
  type RequestInfoResponse,
  type RequestStatusType,
} from "@/lib/api";
import { useAuthedNames } from "@/lib/useAuthedNames";
import { useAccountNames } from "@/lib/useAccountNames";
import { useAuthStore } from "@/stores/auth-store";
import { useRequestColumns } from "@/lib/useRequestColumns";
import { AccountAutocomplete } from "@/components/AccountAutocomplete";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { DataTable } from "@/components/DataTable";
import { RequestDetail } from "@/pages/RequestDetail";

const TABS = ["failed", "pending", "inProgress", "delivering", "done"] as const;
const PLATFORMS = ["telegram", "discord"] as const;
const FILTER_KINDS = ["user", "place"] as const;

const PAGE_SIZE = 50;

function toNumber(v: string | number): number {
  return typeof v === "number" ? v : Number(v);
}

type AccountFilter = {
  filterKind: (typeof FILTER_KINDS)[number];
  filterPlatform: AccountPlatform;
  filterId: string;
};

function normalizeFilterId(id: string): string {
  const trimmed = id.trim();
  if (trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"')) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function parseAccountFilter(params: {
  filterKind: string | null;
  filterPlatform: string | null;
  filterId: string;
}): AccountFilter | null {
  const filterId = normalizeFilterId(params.filterId);
  if (
    (params.filterKind === "user" || params.filterKind === "place") &&
    (params.filterPlatform === "telegram" ||
      params.filterPlatform === "discord") &&
    filterId.length > 0
  ) {
    return {
      filterKind: params.filterKind,
      filterPlatform: params.filterPlatform,
      filterId,
    };
  }
  return null;
}

export function RequestsPage() {
  const [{ tab, cursor, filterKind, filterPlatform, filterId }, setParams] =
    useQueryStates({
      tab: parseAsStringLiteral(TABS).withDefault("failed"),
      cursor: parseAsString.withDefault(""),
      filterKind: parseAsStringLiteral(FILTER_KINDS),
      filterPlatform: parseAsStringLiteral(PLATFORMS),
      filterId: parseAsString.withDefault(""),
    });
  const [selected, setSelected] = useState<RequestInfoResponse | null>(null);
  const qc = useQueryClient();
  const { name: authedName } = useAuthedNames();
  const accounts = useAccountNames();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);

  const filter = parseAccountFilter({
    filterKind,
    filterPlatform,
    filterId,
  });

  const normalizedFilterId = normalizeFilterId(filterId);

  const accountUsers = useQuery({
    queryKey: ["accounts", "users"],
    queryFn: () => api.listAccountUsers(),
    enabled: filterKind === "user" || filterKind === "place",
    staleTime: 30_000,
  });
  const accountPlaces = useQuery({
    queryKey: ["accounts", "places"],
    queryFn: () => api.listAccountPlaces(),
    enabled: filterKind === "user" || filterKind === "place",
    staleTime: 30_000,
  });

  const counts = useQuery({
    queryKey: ["request-counts"],
    queryFn: () => api.requestCounts(),
    refetchInterval: 10_000,
  });

  const list = useInfiniteQuery({
    queryKey: filter
      ? [
          "requests",
          "by-account",
          filter.filterKind,
          filter.filterPlatform,
          filter.filterId,
          tab,
        ]
      : ["requests", tab],
    queryFn: ({ pageParam }) => {
      const opts = { limit: PAGE_SIZE, cursor: pageParam || undefined };
      if (filter) {
        return filter.filterKind === "user"
          ? api.listRequestsByUser(filter.filterPlatform, filter.filterId, {
              ...opts,
              status: tab,
            })
          : api.listRequestsByPlace(filter.filterPlatform, filter.filterId, {
              ...opts,
              status: tab,
            });
      }
      return api.listRequests(tab, opts);
    },
    initialPageParam: cursor || "",
    getNextPageParam: (last) => (last.isDone ? undefined : last.continueCursor),
    staleTime: 5_000,
  });

  const rows = useMemo<RequestInfoResponse[]>(
    () => list.data?.pages.flatMap((p) => p.page) ?? [],
    [list.data],
  );

  function changeTab(next: RequestStatusType) {
    if (next === tab) return;
    void setParams({ tab: next, cursor: "" });
  }

  function loadMore() {
    void list.fetchNextPage();
  }

  function clearFilter() {
    void setParams({
      filterKind: null,
      filterPlatform: null,
      filterId: "",
      cursor: "",
    });
  }

  function invalidateRequests() {
    void qc.invalidateQueries({ queryKey: ["requests"] });
  }

  const retry = useMutation({
    mutationFn: (id: string) => api.retryRequest(id),
    onSuccess: invalidateRequests,
  });
  const cancel = useMutation({
    mutationFn: (id: string) => api.cancelRequest(id),
    onSuccess: invalidateRequests,
  });
  const remove = useMutation({
    mutationFn: (id: string) => api.removeRequest(id),
    onSuccess: () => {
      invalidateRequests();
      setSelected(null);
    },
  });
  const clearRefusals = useMutation({
    mutationFn: (id: string) => api.clearRefusals(id),
    onSuccess: invalidateRequests,
  });

  function confirmAction(action: string, fn: () => void) {
    if (window.confirm(`${action}? This cannot be undone.`)) fn();
  }

  function authedLabel(id: string | undefined): string {
    if (!id) return "—";
    const n = authedName(id);
    return n ? `${n} (${id.slice(-6)})` : id.slice(-12);
  }

  const filterLabel = useMemo(() => {
    if (!filter) return null;
    if (filter.filterKind === "user") {
      return accounts.userLabelWithFallback({
        platform: filter.filterPlatform,
        id: filter.filterId,
      });
    }
    return accounts.placeLabelWithFallback({
      platform: filter.filterPlatform,
      id: filter.filterId,
    });
  }, [filter, accounts]);

  const columns = useRequestColumns({
    accounts,
    authedLabel,
    readonly,
    retry,
    cancel,
    remove,
    confirmAction,
  });

  const platform = filterPlatform ?? "telegram";

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-sm">Account filter</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-xs">
            <span className="text-muted-foreground">Platform</span>
            <select
              className="h-9 rounded-md border bg-background px-2 text-sm"
              value={platform}
              onChange={(e) =>
                void setParams({
                  filterPlatform: e.target.value as AccountPlatform,
                  filterId: "",
                  cursor: "",
                })
              }
            >
              {PLATFORMS.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          </label>
          <div className="flex gap-1">
            {FILTER_KINDS.map((kind) => (
              <Button
                key={kind}
                size="sm"
                variant={filterKind === kind ? "default" : "outline"}
                onClick={() =>
                  void setParams({ filterKind: kind, filterId: "", cursor: "" })
                }
              >
                {kind === "user" ? "User" : "Place"}
              </Button>
            ))}
          </div>
          {filterKind && (
            <label className="min-w-[220px] flex-1 flex flex-col gap-1 text-xs">
              <span className="text-muted-foreground">
                {filterKind === "user" ? "User" : "Place"} ID
              </span>
              <AccountAutocomplete
                platform={platform}
                kind={filterKind}
                users={accountUsers.data}
                places={accountPlaces.data}
                value={normalizedFilterId}
                onChange={(id) => void setParams({ filterId: id, cursor: "" })}
              />
            </label>
          )}
          {filter && (
            <Button size="sm" variant="ghost" onClick={clearFilter}>
              Clear filter
            </Button>
          )}
        </CardContent>
      </Card>

      {filter && filterLabel && (
        <p className="text-sm text-muted-foreground">
          Showing <span className="capitalize">{tab}</span> requests for{" "}
          <span className="font-medium text-foreground">{filterLabel}</span> (
          {filter.filterPlatform}:{filter.filterId})
        </p>
      )}

      <div className="flex gap-1">
        {TABS.map((t) => (
          <Button
            key={t}
            variant={tab === t ? "default" : "outline"}
            size="sm"
            onClick={() => changeTab(t)}
          >
            <span className="capitalize">{t}</span>
            {counts.data ? ` (${toNumber(counts.data[t])})` : ""}
          </Button>
        ))}
      </div>

      <div className="grid gap-4 lg:grid-cols-[1fr_360px]">
        <Card>
          <CardHeader>
            <CardTitle className="capitalize">{tab} requests</CardTitle>
          </CardHeader>
          <CardContent>
            {list.isLoading ? (
              <p className="text-muted-foreground">Loading…</p>
            ) : (
              <>
                <DataTable
                  columns={columns}
                  data={rows}
                  onRowClick={(r) => setSelected(r)}
                  emptyMessage={
                    filter
                      ? `No ${tab} requests for this ${filter.filterKind}.`
                      : `No ${tab} requests.`
                  }
                />
                <div className="mt-3 flex justify-center">
                  {list.isFetchingNextPage ? (
                    <span className="text-xs text-muted-foreground">
                      Loading more…
                    </span>
                  ) : list.hasNextPage ? (
                    <Button size="sm" variant="outline" onClick={loadMore}>
                      Load more
                    </Button>
                  ) : rows.length > 0 ? (
                    <span className="text-xs text-muted-foreground">
                      End of list.
                    </span>
                  ) : null}
                </div>
              </>
            )}
          </CardContent>
        </Card>

        <RequestDetail
          request={selected}
          onClose={() => setSelected(null)}
          authedLabel={authedLabel}
          onRetry={(id) => retry.mutate(id)}
          onCancel={(id) => cancel.mutate(id)}
          onClearRefusals={(id) => clearRefusals.mutate(id)}
          onDelete={(id) =>
            confirmAction("Delete request", () => remove.mutate(id))
          }
        />
      </div>
    </div>
  );
}
