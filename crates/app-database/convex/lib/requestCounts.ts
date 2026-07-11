import { TableAggregate } from "@convex-dev/aggregate";
import type { DataModel, Doc } from "../_generated/dataModel";
import { components } from "../_generated/api";
import { requestsId } from "../schema";

export type RequestStatusNamespace =
  "pending" | "inProgress" | "delivering" | "done" | "failed";

/**
 * Aggregate over the `requests` table, namespaced by `status.Type`.
 * Backs `requests:counts` with O(log n) count lookups instead of scanning
 * every row per status. Kept in sync by the trigger in `./triggers.ts`.
 */
export const requestCountsAggregate = new TableAggregate<{
  Namespace: RequestStatusNamespace;
  Key: number;
  DataModel: DataModel;
  TableName: typeof requestsId;
}>(components.requestCounts, {
  namespace: (doc) => doc.status.Type as RequestStatusNamespace,
  sortKey: (doc) => Number(doc._creationTime),
});

export type RequestDoc = Doc<typeof requestsId>;
