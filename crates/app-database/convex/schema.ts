import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export const authedId = "downloader_hub_authed" as const;
export const authed = {
  name: v.string(),
  token: v.string(),
  readonly: v.boolean(),
  for: v.union(v.literal("worker"), v.literal("bot")),
  onlyTagged: v.optional(v.array(v.string())),
  expiresAt: v.optional(v.int64()),
};

export const requestPending = {
  Type: v.literal("pending"),
};

export const requestInProgress = {
  Type: v.literal("inProgress"),
  since: v.int64(),
  by: v.id(authedId),
  message: v.optional(v.string()),
  filesData: v.optional(v.string()),
  waitingForRequester: v.optional(v.boolean()),
  CleanupId: v.id("_scheduled_functions"),
};

export const requestDone = {
  Type: v.literal("done"),
  at: v.int64(),
  by: v.id(authedId),
};

export const requestFailed = {
  Type: v.literal("failed"),
  at: v.int64(),
  by: v.id(authedId),
  reason: v.string(),
};

export const requestsId = "downloader_hub_requests" as const;
export const requests = {
  requester: v.id(authedId),
  info: v.string(),
  tries: v.int64(),
  // used to deduplicate requests, must be globally unique
  idempotencyKey: v.optional(v.string()),
  status: v.union(
    v.object(requestPending),
    v.object(requestInProgress),
    v.object(requestDone),
    v.object(requestFailed),
  ),
  errors: v.array(v.string()),
  metadata: v.optional(v.record(v.string(), v.string())),
  lastModified: v.int64(),
  refusedBy: v.optional(v.array(v.id(authedId))),
};

export const outboxId = "downloader_hub_outbox" as const;
export const outbox = {
  message: v.union(v.bytes(), v.string()),
  audiences: v.optional(v.bytes()),
  sentBy: v.id(authedId),
};

export const connectionsId = "downloader_hub_connections" as const;
export const connections = {
  central: v.string(),
  authed: v.id(authedId),
  role: v.union(v.literal("worker"), v.literal("bot")),
  capabilities: v.optional(v.string()),
  version: v.optional(v.string()),
  lastSeen: v.int64(),
};

export default defineSchema(
  {
    [authedId]: defineTable(authed).index("by_token", ["token"]),

    [requestsId]: defineTable(requests)
      .index("by_status_type", ["status.Type", "requester"])
      .index("by_idempotency_key", ["idempotencyKey"]),

    [outboxId]: defineTable(outbox).index("by_sentBy", ["sentBy"]),

    [connectionsId]: defineTable(connections)
      .index("by_central_authed", ["central", "authed"])
      .index("by_last_seen", ["lastSeen"]),
  },
  {
    schemaValidation: true,
    strictTableNameTypes: false,
  },
);
