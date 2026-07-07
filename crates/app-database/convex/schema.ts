import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

export const authedId = "downloader_hub_authed" as const;
export const authed = {
  name: v.string(),
  token: v.string(),
  readonly: v.boolean(),
  for: v.union(v.literal("worker"), v.literal("bot"), v.literal("admin")),
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

export const accountPlatform = v.union(
  v.literal("telegram"),
  v.literal("discord"),
);

export const accountUserRef = v.object({
  platform: accountPlatform,
  id: v.string(),
});

export const accountPlaceRef = v.object({
  platform: accountPlatform,
  id: v.string(),
});

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
  // end-user who ordered the download (the bot itself is `requester`).
  // both are optional so legacy rows keep validating.
  orderedBy: v.optional(accountUserRef),
  // chat/server/channel the request originated from.
  orderedIn: v.optional(accountPlaceRef),
};

export const accountUserId = "downloader_hub_account_users" as const;
export const accountUser = {
  platform: accountPlatform,
  platformId: v.string(),
  username: v.optional(v.string()),
  displayName: v.optional(v.string()),
  isBot: v.optional(v.boolean()),
  lastSeen: v.int64(),
};

export const accountPlaceId = "downloader_hub_account_places" as const;
export const accountPlace = {
  platform: accountPlatform,
  platformId: v.string(),
  kind: v.optional(v.string()),
  name: v.optional(v.string()),
  username: v.optional(v.string()),
  // discord guild id when this place is a guild channel; absent for DMs and for the guild row itself.
  parentPlatformId: v.optional(v.string()),
  lastSeen: v.int64(),
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
  role: v.union(v.literal("worker"), v.literal("bot"), v.literal("admin")),
  capabilities: v.optional(v.string()),
  version: v.optional(v.string()),
  lastSeen: v.int64(),
};

export default defineSchema(
  {
    [authedId]: defineTable(authed).index("by_token", ["token"]),

    [requestsId]: defineTable(requests)
      .index("by_status_type", ["status.Type", "requester"])
      .index("by_status_creation", ["status.Type"])
      .index("by_idempotency_key", ["idempotencyKey"])
      .index("by_last_modified", ["lastModified"]),

    [outboxId]: defineTable(outbox).index("by_sentBy", ["sentBy"]),

    [connectionsId]: defineTable(connections)
      .index("by_central_authed", ["central", "authed"])
      .index("by_last_seen", ["lastSeen"]),

    [accountUserId]: defineTable(accountUser).index("by_platform_id", [
      "platform",
      "platformId",
    ]),

    [accountPlaceId]: defineTable(accountPlace).index("by_platform_id", [
      "platform",
      "platformId",
    ]),
  },
  {
    schemaValidation: true,
    strictTableNameTypes: false,
  },
);
