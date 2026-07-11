import { useMemo, useState } from "react";
import { useInfiniteQuery, useMutation, useQueryClient } from "@tanstack/react-query";
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
import { Button } from "@/components/ui/button";
import { DataTable } from "@/components/DataTable";
import { RequestDetail } from "@/pages/RequestDetail";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

const PAGE_SIZE = 50;

export interface AccountRequestsListProps {
  kind: "user" | "place";
  platform: AccountPlatform;
  accountId: string;
  status?: RequestStatusType;
  emptyMessage?: string;
}

function accountQueryKey(
  kind: "user" | "place",
  platform: AccountPlatform,
  accountId: string,
  status?: RequestStatusType,
) {
  return ["requests", "by-account", kind, platform, accountId, status ?? "all"];
}

export function AccountRequestsList({
  kind,
  platform,
  accountId,
  status,
  emptyMessage,
}: AccountRequestsListProps) {
  const [selected, setSelected] = useState<RequestInfoResponse | null>(null);
  const qc = useQueryClient();
  const { name: authedName } = useAuthedNames();
  const accounts = useAccountNames();
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);

  const list = useInfiniteQuery({
    queryKey: accountQueryKey(kind, platform, accountId, status),
    queryFn: ({ pageParam }) => {
      const opts = {
        limit: PAGE_SIZE,
        cursor: pageParam || undefined,
        status,
      };
      return kind === "user"
        ? api.listRequestsByUser(platform, accountId, opts)
        : api.listRequestsByPlace(platform, accountId, opts);
    },
    initialPageParam: "",
    getNextPageParam: (last) => (last.isDone ? undefined : last.continueCursor),
    staleTime: 5_000,
  });

  const rows = useMemo<RequestInfoResponse[]>(
    () => list.data?.pages.flatMap((p) => p.page) ?? [],
    [list.data],
  );

  function invalidate() {
    void qc.invalidateQueries({ queryKey: ["requests"] });
  }

  const retry = useMutation({
    mutationFn: (id: string) => api.retryRequest(id),
    onSuccess: invalidate,
  });
  const cancel = useMutation({
    mutationFn: (id: string) => api.cancelRequest(id),
    onSuccess: invalidate,
  });
  const remove = useMutation({
    mutationFn: (id: string) => api.removeRequest(id),
    onSuccess: () => {
      invalidate();
      setSelected(null);
    },
  });
  const clearRefusals = useMutation({
    mutationFn: (id: string) => api.clearRefusals(id),
    onSuccess: invalidate,
  });

  function confirmAction(action: string, fn: () => void) {
    if (window.confirm(`${action}? This cannot be undone.`)) fn();
  }

  function authedLabel(id: string | undefined): string {
    if (!id) return "—";
    const n = authedName(id);
    return n ? `${n} (${id.slice(-6)})` : id.slice(-12);
  }

  const columns = useRequestColumns({
    accounts,
    authedLabel,
    readonly,
    retry,
    cancel,
    remove,
    confirmAction,
    showStatus: !status,
  });

  const defaultEmpty =
    "No requests found. Legacy rows may lack orderedBy/orderedIn refs until backfilled.";

  return (
    <>
      {list.isLoading ? (
        <p className="text-muted-foreground">Loading…</p>
      ) : (
        <>
          <DataTable
            columns={columns}
            data={rows}
            onRowClick={(r) => setSelected(r)}
            emptyMessage={emptyMessage ?? defaultEmpty}
          />
          <div className="mt-3 flex justify-center">
            {list.isFetchingNextPage ? (
              <span className="text-xs text-muted-foreground">Loading more…</span>
            ) : list.hasNextPage ? (
              <Button
                size="sm"
                variant="outline"
                onClick={() => void list.fetchNextPage()}
              >
                Load more
              </Button>
            ) : rows.length > 0 ? (
              <span className="text-xs text-muted-foreground">End of list.</span>
            ) : null}
          </div>
        </>
      )}

      <Dialog open={selected !== null} onOpenChange={(open) => !open && setSelected(null)}>
        <DialogContent className="max-h-[85vh] max-w-lg overflow-y-auto">
          <DialogHeader>
            <DialogTitle>Request detail</DialogTitle>
          </DialogHeader>
          {selected && (
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
              embedded
            />
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}
