import { Triggers } from "convex-helpers/server/triggers";
import type { DataModel } from "../_generated/dataModel";
import { requestsId } from "../schema";
import { requestCountsAggregate } from "./requestCounts";

const triggers = new Triggers<DataModel>();

// Keep the requestCounts aggregate in sync with the `requests` table.
// Fires on every ctx.db.insert/patch/replace/delete against the table,
// including status transitions (e.g. pending → inProgress via `take`).
// `patch` is surfaced as an `update` with both oldDoc and newDoc; the
// aggregate's `replace` reconciles the namespace when status.Type changes.
triggers.register(requestsId, async (ctx, change) => {
  switch (change.operation) {
    case "insert":
      await requestCountsAggregate.insert(ctx, change.newDoc);
      break;
    case "update":
      await requestCountsAggregate.replace(ctx, change.oldDoc, change.newDoc);
      break;
    case "delete":
      await requestCountsAggregate.delete(ctx, change.oldDoc);
      break;
  }
});

export default triggers;
