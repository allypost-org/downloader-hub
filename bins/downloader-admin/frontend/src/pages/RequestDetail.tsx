import { useQuery } from "@tanstack/react-query";
import type { AccountRef, RequestInfoResponse } from "@/lib/api";
import { api } from "@/lib/api";
import { useAccountNames } from "@/lib/useAccountNames";
import { useAuthStore } from "@/stores/auth-store";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { StatusBadge } from "@/components/StatusBadge";

interface Props {
  request: RequestInfoResponse | null;
  onClose: () => void;
  authedLabel: (id: string | undefined) => string;
  onRetry: (id: string) => void;
  onCancel: (id: string) => void;
  onClearRefusals: (id: string) => void;
  onDelete: (id: string) => void;
}

function formatTime(ms: string | number): string {
  const n = typeof ms === "number" ? ms : Number(ms);
  if (!Number.isFinite(n) || n <= 0) return "—";
  return new Date(n).toLocaleString();
}

export function RequestDetail({
  request,
  onClose,
  authedLabel,
  onRetry,
  onCancel,
  onClearRefusals,
  onDelete,
}: Props) {
  const readonly = useAuthStore((s) => s.me?.readonly ?? false);
  const accounts = useAccountNames();

  // Refetch the request by id while the detail is open, so status transitions
  // (pending → inProgress → done) appear without re-clicking. The prop is the
  // initial snapshot (from the list row); the query keeps it fresh.
  const { data } = useQuery({
    queryKey: ["request", request?.requestId],
    queryFn: () => api.getRequest(request!.requestId),
    enabled: !!request?.requestId,
    refetchInterval: 5_000,
    initialData: request ?? undefined,
  });

  if (!data) {
    return (
      <Card>
        <CardContent className="p-6 text-sm text-muted-foreground">
          Select a request to see its details.
        </CardContent>
      </Card>
    );
  }

  // `data` is the live-fetched request; alias back to `request` so the rest of
  // the component reads the fresh value rather than the stale prop.
  const req = data;
  const status = req.status;

  function refFieldLabel(ref: AccountRef | null | undefined): string {
    if (!ref) return "—";
    return ref.platform === "telegram"
      ? `telegram:${ref.id}`
      : `discord:${ref.id}`;
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center justify-between text-sm">
          <span>Request detail</span>
          <Button size="sm" variant="ghost" onClick={onClose}>
            Close
          </Button>
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4 text-sm">
        <div>
          <div className="mb-1 text-muted-foreground">ID</div>
          <code className="break-all text-xs">{req.requestId}</code>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-muted-foreground">Status</span>
          <StatusBadge status={status.Type} />
        </div>
        <Field label="From" value={authedLabel(req.requester)} />
        <Field
          label="Ordered by"
          value={
            req.orderedBy
              ? `${accounts.userLabelWithFallback(req.orderedBy)} (${refFieldLabel(req.orderedBy)})`
              : "—"
          }
        />
        <Field
          label="Ordered in"
          value={
            req.orderedIn
              ? `${accounts.placeLabelWithFallback(req.orderedIn)} (${refFieldLabel(req.orderedIn)})`
              : "—"
          }
        />
        {status.by && <Field label="By" value={authedLabel(status.by)} />}
        {status.reason && <Field label="Reason" value={status.reason} />}
        {status.message && <Field label="Message" value={status.message} />}
        {req.idempotencyKey && (
          <Field label="Idempotency key" value={req.idempotencyKey} mono />
        )}
        <Field label="Created" value={formatTime(req.createdAt)} />
        <Field label="Modified" value={formatTime(req.lastModified)} />
        <Field
          label="Refused by"
          value={
            req.refusedBy.length > 0
              ? req.refusedBy.map(authedLabel).join(", ")
              : "—"
          }
        />

        <div className="space-y-1">
          <div className="text-muted-foreground">Info</div>
          <pre className="max-h-40 overflow-auto rounded-md bg-muted p-2 text-xs whitespace-pre-wrap">
            {(() => {
              try {
                return JSON.stringify(req.info, null, 2);
              } catch {
                return String(req.info);
              }
            })()}
          </pre>
        </div>

        <div className="space-y-1">
          <div className="text-muted-foreground">
            Errors ({req.errors.length})
          </div>
          {req.errors.length > 0 ? (
            <ul className="max-h-40 space-y-1 overflow-auto rounded-md bg-muted p-2 text-xs">
              {req.errors.map((e, i) => (
                <li key={i} className="break-all font-mono">
                  {e}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-xs text-muted-foreground">No errors recorded.</p>
          )}
        </div>

        <div className="flex flex-wrap gap-2 pt-2">
          {(status.Type === "failed" || status.Type === "done") && (
            <Button
              size="sm"
              disabled={readonly}
              onClick={() => onRetry(req.requestId)}
            >
              Retry
            </Button>
          )}
          {(status.Type === "pending" || status.Type === "inProgress") && (
            <Button
              size="sm"
              variant="outline"
              disabled={readonly}
              onClick={() => onCancel(req.requestId)}
            >
              Cancel
            </Button>
          )}
          <Button
            size="sm"
            variant="outline"
            disabled={readonly}
            onClick={() => onClearRefusals(req.requestId)}
          >
            Clear refusals
          </Button>
          <Button
            size="sm"
            variant="destructive"
            disabled={readonly}
            onClick={() => onDelete(req.requestId)}
          >
            Delete
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

function Field({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div>
      <div className="text-muted-foreground">{label}</div>
      <div
        className={mono ? "break-all font-mono text-xs" : "break-all text-xs"}
      >
        {value}
      </div>
    </div>
  );
}
