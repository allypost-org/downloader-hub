import { useMemo } from "react";
import { type ColumnDef } from "@tanstack/react-table";
import type { UseMutationResult } from "@tanstack/react-query";
import type { RequestInfoResponse } from "@/lib/api";
import type { useAccountNames } from "@/lib/useAccountNames";
import { Button } from "@/components/ui/button";
import { StatusBadge } from "@/components/StatusBadge";

function toNumber(v: string | number): number {
  return typeof v === "number" ? v : Number(v);
}

function formatTime(ms: string | number): string {
  const n = toNumber(ms);
  if (!Number.isFinite(n) || n <= 0) return "—";
  return new Date(n).toLocaleString();
}

interface UseRequestColumnsOptions {
  accounts: ReturnType<typeof useAccountNames>;
  authedLabel: (id: string | undefined) => string;
  readonly: boolean;
  retry: UseMutationResult<{ retried: boolean }, Error, string>;
  cancel: UseMutationResult<{ cancelled: boolean }, Error, string>;
  remove: UseMutationResult<{ removed: boolean }, Error, string>;
  confirmAction: (action: string, fn: () => void) => void;
  showStatus?: boolean;
}

export function useRequestColumns({
  accounts,
  authedLabel,
  readonly,
  retry,
  cancel,
  remove,
  confirmAction,
  showStatus = false,
}: UseRequestColumnsOptions): ColumnDef<RequestInfoResponse>[] {
  return useMemo(
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
      ...(showStatus
        ? [
            {
              accessorKey: "status.Type",
              header: "Status",
              enableSorting: false,
              cell: ({ row }: { row: { original: RequestInfoResponse } }) => (
                <StatusBadge status={row.original.status.Type} />
              ),
            } satisfies ColumnDef<RequestInfoResponse>,
          ]
        : []),
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
    [
      accounts,
      authedLabel,
      readonly,
      retry,
      cancel,
      remove,
      confirmAction,
      showStatus,
    ],
  );
}
