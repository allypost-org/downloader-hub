import type { AdminParkedWorker, RequestInfoResponse } from "@/lib/api";

export function isRequestParked(
  req: RequestInfoResponse,
  parkedWorkers: AdminParkedWorker[] | undefined,
): boolean {
  if (req.status.Type !== "pending") return false;
  if (!parkedWorkers?.length) return false;
  return parkedWorkers.every((w) => req.refusedBy.includes(w.authedId));
}
