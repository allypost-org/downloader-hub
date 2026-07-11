import { useMemo, useState } from "react";
import { type ColumnDef } from "@tanstack/react-table";
import { useMutation, useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  parseAsString,
  parseAsStringLiteral,
  useQueryStates,
} from "nuqs";
import {
  api,
  type RequestInfoResponse,
  type RequestStatusType,
} from "@/lib/api";
import { useAuthedNames } from "@/lib/useAuthedNames";
import { useAccountNames } from "@/lib/useAccountNames";
import { useAuthStore } from "@/stores/auth-store";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { DataTable } from "@/components/DataTable";
import { StatusBadge } from "@/components/StatusBadge";
import { RequestDetail } from "@/pages/RequestDetail";

const TABS = ["failed", "pending", "inProgress", "delivering", "done"] as const;

const PAGE_SIZE = 50;

function toNumber(v: string | number): number {
  return typeof v === "number" ? v : Number(v);
}

function formatTime(ms: string | number): string {
  const n = toNumber(ms);
  if (!Number.isFinite(n) || n <= 0) return "—";
  return new Date(n).toLocaleString();
}

export function RequestsPage() {
  const [{ tab, cursor }, setParams] = useQueryStates({
    tab: parseAsStringLiteral(TABS).withDefault("failed"),
    cursor: parseAsString.withDefault(""),
  });
  const [selected, setSelected] = useState<RequestInfoResponse | null>(null);
  const qc = useQueryClient();
  const { name: authedName } = useAuthedNames();
  const accounts = useAccountNames();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);

  const counts = useQuery({
    queryKey: ["request-counts"],
    queryFn: () => api.requestCounts(),
    refetchInterval: 10_000,
  });

  const list = useInfiniteQuery({
    queryKey: ["requests", tab],
    queryFn: ({ pageParam }) =>
      api.listRequests(tab, { limit: PAGE_SIZE, cursor: pageParam || undefined }),
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

  const retry = useMutation({
    mutationFn: (id: string) => api.retryRequest(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["requests"] }),
  });
  const cancel = useMutation({
    mutationFn: (id: string) => api.cancelRequest(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["requests"] }),
  });
  const remove = useMutation({
    mutationFn: (id: string) => api.removeRequest(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["requests"] });
      setSelected(null);
    },
  });
  const clearRefusals = useMutation({
    mutationFn: (id: string) => api.clearRefusals(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["requests"] }),
  });

  function confirmAction(action: string, fn: () => void) {
    if (window.confirm(`${action}? This cannot be undone.`)) fn();
  }

  function authedLabel(id: string | undefined): string {
    if (!id) return "—";
    const n = authedName(id);
    return n ? `${n} (${id.slice(-6)})` : id.slice(-12);
  }

  const columns = useMemo<ColumnDef<RequestInfoResponse>[]>(
    () => [
      {
        accessorFn: (r) => r.requestId.slice(-8),
        id: "id",
        header: "ID",
        cell: ({ row }) => (
          <span className="font-mono text-xs">
            {row.original.requestId.slice(-8)}
          </span>
        ),
      },
      {
        accessorKey: "requester",
        header: "From",
        cell: ({ row }) => (
          <span className="text-xs">
            {row.original.requesterName ?? authedLabel(row.original.requester)}
          </span>
        ),
      },
      {
        id: "orderedBy",
        header: "Ordered by",
        accessorFn: (r) =>
          r.orderedBy ? accounts.userLabelWithFallback(r.orderedBy) : "",
        cell: ({ row }) => {
          const r = row.original;
          return (
            <div className="flex flex-col text-xs">
              <span>
                {r.orderedBy
                  ? accounts.userLabelWithFallback(r.orderedBy)
                  : "—"}
              </span>
              {r.orderedIn && (
                <span className="text-muted-foreground">
                  in {accounts.placeLabelWithFallback(r.orderedIn)}
                </span>
              )}
            </div>
          );
        },
      },
      {
        accessorKey: "status.Type",
        header: "Status",
        enableSorting: false,
        cell: ({ row }) => <StatusBadge status={row.original.status.Type} />,
      },
      {
        accessorKey: "lastModified",
        header: "Modified",
        cell: ({ row }) => (
          <span className="text-xs text-muted-foreground">
            {formatTime(row.original.lastModified)}
          </span>
        ),
      },
      {
        id: "actions",
        header: () => <div className="text-right">Actions</div>,
        enableSorting: false,
        cell: ({ row }) => {
          const r = row.original;
          return (
            <div
              className="flex justify-end gap-1 text-right"
              onClick={(e) => e.stopPropagation()}
            >
              {(r.status.Type === "failed" || r.status.Type === "done") && (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={readonly || retry.isPending}
                  onClick={() => retry.mutate(r.requestId)}
                >
                  Retry
                </Button>
              )}
              {(r.status.Type === "pending" ||
                r.status.Type === "inProgress" ||
                r.status.Type === "delivering") && (
                <Button
                  size="sm"
                  variant="outline"
                  disabled={readonly || cancel.isPending}
                  onClick={() => cancel.mutate(r.requestId)}
                >
                  Cancel
                </Button>
              )}
              <Button
                size="sm"
                variant="destructive"
                disabled={readonly || remove.isPending}
                onClick={() =>
                  confirmAction("Delete request", () =>
                    remove.mutate(r.requestId),
                  )
                }
              >
                Delete
              </Button>
            </div>
          );
        },
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [accounts, authedName, readonly, retry, cancel, remove],
  );

  return (
    <div className="space-y-4">
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
                  emptyMessage={`No ${tab} requests.`}
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
