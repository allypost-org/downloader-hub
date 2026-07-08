import { v } from "convex/values";
import {
  internalMutation as rawInternalMutation,
  mutation as rawMutation,
  query,
} from "./_generated/server";
import { internal } from "./_generated/api";
import schema, {
  accountPlaceRef,
  accountUserRef,
  authedId,
  requestPending,
  requestFailed,
  requestInProgress,
  requests,
  requestsId,
} from "./schema";
import { Doc, Id } from "./_generated/dataModel";
import { mergedStream, stream } from "convex-helpers/server/stream";
import {
  customCtx,
  customMutation,
} from "convex-helpers/server/customFunctions";
import triggers from "./lib/triggers";
import { requestCountsAggregate } from "./lib/requestCounts";

// Wrap every mutation in the requestCounts trigger so the aggregate backing
// `counts` stays in sync with the `requests` table automatically.
const mutation = customMutation(rawMutation, customCtx(triggers.wrapDB));
const internalMutation = customMutation(
  rawInternalMutation,
  customCtx(triggers.wrapDB),
);

// const MAX_PROCESSING_TIME_MS = 15 * 60 * 1_000;
// const MAX_PROCESSING_TIME_MS = 15 * 1_000;
const MAX_PROCESSING_IDLE_TIME_MS = 10 * 60 * 1_000;
const MAX_WAITING_FOR_REQUESTER_IDLE_TIME_MS = 15 * 60 * 1_000;
const MAX_TRIES = 5;

export const get = query({
  args: {
    requestId: v.id(requestsId),
  },
  handler: (ctx, args) => {
    return ctx.db.get(args.requestId);
  },
});

export const add = mutation({
  args: {
    info: requests.info,
    requesterId: requests.requester,
    metadata: requests.metadata,
    idempotencyKey: requests.idempotencyKey,
    orderedBy: v.optional(accountUserRef),
    orderedIn: v.optional(accountPlaceRef),
  },
  returns: v.object({
    requestId: v.id(requestsId),
  }),
  handler: async (ctx, args) => {
    if (args.idempotencyKey) {
      const existing = await ctx.db
        .query(requestsId)
        .withIndex("by_idempotency_key", (q) =>
          q.eq("idempotencyKey", args.idempotencyKey),
        )
        .first();

      if (existing) {
        console.log(
          args.idempotencyKey,
          "already exists, returning existing request",
        );

        return {
          requestId: existing._id,
        };
      }
    }

    const id = await ctx.db.insert(requestsId, {
      info: args.info,
      requester: args.requesterId,
      metadata: args.metadata,
      tries: 0n,
      idempotencyKey: args.idempotencyKey,
      status: {
        Type: "pending",
      },
      lastModified: BigInt(Date.now()),
      errors: [],
      orderedBy: args.orderedBy,
      orderedIn: args.orderedIn,
    });

    return {
      requestId: id,
    };
  },
});

const requestDataReturn = {
  requestId: v.id(requestsId),
  requester: requests.requester,
  info: requests.info,
  metadata: requests.metadata,
  status: requests.status,
  errors: requests.errors,
  refusedBy: requests.refusedBy,
  idempotencyKey: requests.idempotencyKey,
  lastModified: requests.lastModified,
  createdAt: v.number(),
  orderedBy: requests.orderedBy,
  orderedIn: requests.orderedIn,
};

export const getFirstAvailable = query({
  args: {},
  returns: v.union(v.object(requestDataReturn), v.null()),
  handler: async (ctx) => {
    const row = await ctx.db
      .query(requestsId)
      .withIndex("by_status_type", (q) => q.eq("status.Type", "pending"))
      .order("asc")
      .first();

    if (!row) {
      return null;
    }

    return {
      requestId: row._id,
      requester: row.requester,
      info: row.info,
      metadata: row.metadata,
      status: row.status,
      errors: row.errors,
      refusedBy: row.refusedBy ?? [],
      idempotencyKey: row.idempotencyKey,
      lastModified: row.lastModified,
      createdAt: row._creationTime,
      orderedBy: row.orderedBy,
      orderedIn: row.orderedIn,
    };
  },
});

export const getAllAvailable = query({
  args: {},
  returns: v.array(v.object(requestDataReturn)),
  handler: async (ctx) => {
    const rows = await ctx.db
      .query(requestsId)
      .withIndex("by_status_type", (q) => q.eq("status.Type", "pending"))
      .order("asc")
      .collect();

    return rows.map((row) => ({
      requestId: row._id,
      requester: row.requester,
      info: row.info,
      metadata: row.metadata,
      status: row.status,
      errors: row.errors,
      refusedBy: row.refusedBy ?? [],
      idempotencyKey: row.idempotencyKey,
      lastModified: row.lastModified,
      createdAt: row._creationTime,
      orderedBy: row.orderedBy,
      orderedIn: row.orderedIn,
    }));
  },
});

const FAILED_GRACE_WINDOW_MS = 60_000;

export const getMineInProgress = query({
  args: {
    authedId: v.id(authedId),
  },
  returns: v.array(v.object(requestDataReturn)),
  handler: async (ctx, args) => {
    const inStatuses = [
      requestInProgress.Type.value,
      requestPending.Type.value,
    ].map((s) =>
      stream(ctx.db, schema)
        .query(requestsId)
        .withIndex("by_status_type", (q) => q.eq("status.Type", s)),
    );

    const rows = await mergedStream(inStatuses, [
      "status.Type",
      "requester",
      "_creationTime",
    ])
      .filterWith(async (row) => row.requester === args.authedId)
      .collect();

    const failedCutoff = BigInt(Date.now() - FAILED_GRACE_WINDOW_MS);
    const recentFailed = (
      await ctx.db
        .query(requestsId)
        .withIndex("by_status_type", (q) =>
          q
            .eq("status.Type", requestFailed.Type.value)
            .eq("requester", args.authedId),
        )
        .collect()
    ).filter(
      (row) => row.status.Type === "failed" && row.status.at >= failedCutoff,
    );

    return [...rows, ...recentFailed].map((row) => ({
      requestId: row._id,
      requester: row.requester,
      info: row.info,
      metadata: row.metadata,
      status: row.status,
      errors: row.errors,
      refusedBy: row.refusedBy ?? [],
      idempotencyKey: row.idempotencyKey,
      lastModified: row.lastModified,
      createdAt: row._creationTime,
      orderedBy: row.orderedBy,
      orderedIn: row.orderedIn,
    }));
  },
});

export const getInfo = query({
  args: {
    requestId: v.id(requestsId),
  },
  handler: (ctx, args) => {
    return ctx.db.get(args.requestId);
  },
});

export const take = mutation({
  args: {
    requestId: v.id(requestsId),
    takerId: requestInProgress.by,
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestAlreadyTaken"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestAlreadyFulfilled"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
      ...requestDataReturn,
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return {
        ok: false,
        code: "RequestNotFound",
      } as const;
    }

    if (row.status.Type !== "pending") {
      return {
        ok: false,
        code: "RequestAlreadyTaken",
      } as const;
    }

    const tries = row.tries + 1n;

    const scheduledId = (await ctx.scheduler.runAfter(
      MAX_PROCESSING_IDLE_TIME_MS,
      internal.requests.takeCleanup,
      {
        requestId: args.requestId,
        tries,
      },
    )) as Id<"_scheduled_functions">;

    const status = {
      Type: "inProgress",
      since: BigInt(Date.now()),
      by: args.takerId,
      CleanupId: scheduledId,
    } as const;

    await ctx.db.patch(args.requestId, {
      status,
      tries,
      lastModified: BigInt(Date.now()),
    });

    return {
      ok: true,
      code: "Ok",
      requestId: row._id,
      requester: row.requester,
      info: row.info,
      metadata: row.metadata,
      status,
      errors: row.errors,
      refusedBy: row.refusedBy ?? [],
      idempotencyKey: row.idempotencyKey,
      lastModified: row.lastModified,
      createdAt: row._creationTime,
      orderedBy: row.orderedBy,
      orderedIn: row.orderedIn,
    } as const;
  },
});

export const takeCleanup = internalMutation({
  args: {
    requestId: v.id(requestsId),
    tries: requests.tries,
  },
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      console.log(args.requestId, "not found");
      return null;
    }

    if (row.status.Type !== "inProgress") {
      console.log(args.requestId, "not processing");
      return null;
    }

    if (row.tries !== args.tries) {
      console.log(
        args.requestId,
        `not at the expected try: ${args.tries} (expected) vs ${row.tries} (actual)`,
      );
      return null;
    }

    if (row.tries > MAX_TRIES) {
      console.log(args.requestId, "max tries reached");
      await ctx.db.patch(args.requestId, {
        status: {
          Type: "failed",
          at: BigInt(Date.now()),
          by: row.status.by,
          reason: "max tries reached",
        },
        lastModified: BigInt(Date.now()),
        errors: [],
      });
      return null;
    }

    console.log(args.requestId, "cleaning up");
    await ctx.db.patch(args.requestId, {
      status: {
        Type: "pending",
      },
      lastModified: BigInt(Date.now()),
      errors: [],
    });
  },
});

export const updateStatusMessage = mutation({
  args: {
    requestId: v.id(requestsId),
    authedId: v.id(authedId),
    statusMessage: requestInProgress.message,
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotInProgress"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotTakenByYou"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return {
        ok: false,
        code: "RequestNotFound",
      } as const;
    }

    if (row.status.Type !== "inProgress") {
      return {
        ok: false,
        code: "RequestNotInProgress",
      } as const;
    }

    if (row.status.by !== args.authedId) {
      return {
        ok: false,
        code: "RequestNotTakenByYou",
      } as const;
    }

    await ctx.db.patch(args.requestId, {
      status: {
        ...row.status,
        message: args.statusMessage,
      },
      lastModified: BigInt(Date.now()),
    });

    return {
      ok: true,
      code: "Ok",
    } as const;
  },
});

export const addErrors = mutation({
  args: {
    requestId: v.id(requestsId),
    authedId: v.id(authedId),
    errors: v.array(v.string()),
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotInProgress"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotTakenByYou"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return {
        ok: false,
        code: "RequestNotFound",
      } as const;
    }

    if (row.status.Type !== "inProgress") {
      return {
        ok: false,
        code: "RequestNotInProgress",
      } as const;
    }

    if (row.status.by !== args.authedId) {
      return {
        ok: false,
        code: "RequestNotTakenByYou",
      } as const;
    }

    await ctx.db.patch(args.requestId, {
      errors: [...row.errors, ...args.errors],
      lastModified: BigInt(Date.now()),
    });

    return {
      ok: true,
      code: "Ok",
    } as const;
  },
});

export const moveToWaitingForRequester = mutation({
  args: {
    requestId: v.id(requestsId),
    authedId: v.id(authedId),
    filesData: requestInProgress.filesData,
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotInProgress"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotTakenByYou"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return {
        ok: false,
        code: "RequestNotFound",
      } as const;
    }

    if (row.status.Type !== "inProgress") {
      return {
        ok: false,
        code: "RequestNotInProgress",
      } as const;
    }

    if (row.status.by !== args.authedId) {
      return {
        ok: false,
        code: "RequestNotTakenByYou",
      } as const;
    }

    const cleanupId = (await ctx.scheduler.runAfter(
      MAX_WAITING_FOR_REQUESTER_IDLE_TIME_MS,
      internal.requests.takeCleanup,
      {
        requestId: args.requestId,
        tries: row.tries,
      },
    )) as Id<"_scheduled_functions">;

    await Promise.all([
      ctx.db.patch(args.requestId, {
        status: {
          ...row.status,
          waitingForRequester: true,
          filesData: args.filesData,
          CleanupId: cleanupId,
        },
        lastModified: BigInt(Date.now()),
      }),

      ctx.scheduler.cancel(row.status.CleanupId),
    ]);

    return {
      ok: true,
      code: "Ok",
    } as const;
  },
});

export const free = mutation({
  args: {
    requestId: v.id(requestsId),
    takerId: requestInProgress.by,
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotInProgress"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotTakenByYou"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return {
        ok: false,
        code: "RequestNotFound",
      } as const;
    }

    if (row.status.Type !== "inProgress") {
      return {
        ok: false,
        code: "RequestNotInProgress",
      } as const;
    }

    if (row.status.by !== args.takerId) {
      return {
        ok: false,
        code: "RequestNotTakenByYou",
      } as const;
    }

    if (row.tries > MAX_TRIES) {
      console.log(args.requestId, "max tries reached");

      await Promise.all([
        ctx.db.patch(args.requestId, {
          status: {
            Type: "failed",
            at: BigInt(Date.now()),
            by: row.status.by,
            reason: "max tries reached",
          },
          lastModified: BigInt(Date.now()),
          errors: [],
        }),

        ctx.scheduler.cancel(row.status.CleanupId),
      ]);

      return {
        ok: true,
        code: "Ok",
      } as const;
    }

    await Promise.all([
      ctx.db.patch(args.requestId, {
        status: {
          Type: "pending",
        },
        lastModified: BigInt(Date.now()),
        errors: [],
      }),

      ctx.scheduler.cancel(row.status.CleanupId),
    ]);

    return {
      ok: true,
      code: "Ok",
    } as const;
  },
});

export const refuse = mutation({
  args: {
    requestId: v.id(requestsId),
    takerId: requestInProgress.by,
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotInProgress"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotTakenByYou"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return { ok: false, code: "RequestNotFound" } as const;
    }

    if (row.status.Type !== "inProgress") {
      return { ok: false, code: "RequestNotInProgress" } as const;
    }

    if (row.status.by !== args.takerId) {
      return { ok: false, code: "RequestNotTakenByYou" } as const;
    }

    const tries = row.tries > 0n ? row.tries - 1n : 0n;
    const refusedBy = row.refusedBy?.includes(args.takerId)
      ? row.refusedBy
      : [...(row.refusedBy ?? []), args.takerId];

    await Promise.all([
      ctx.db.patch(args.requestId, {
        status: { Type: "pending" },
        tries,
        refusedBy,
        lastModified: BigInt(Date.now()),
        errors: [],
      }),
      ctx.scheduler.cancel(row.status.CleanupId),
    ]);

    return { ok: true, code: "Ok" } as const;
  },
});

export const release = mutation({
  args: {
    requestId: v.id(requestsId),
    takerId: requestInProgress.by,
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotInProgress"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotTakenByYou"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return { ok: false, code: "RequestNotFound" } as const;
    }

    if (row.status.Type !== "inProgress") {
      return { ok: false, code: "RequestNotInProgress" } as const;
    }

    if (row.status.by !== args.takerId) {
      return { ok: false, code: "RequestNotTakenByYou" } as const;
    }

    const tries = row.tries > 0n ? row.tries - 1n : 0n;

    await Promise.all([
      ctx.db.patch(args.requestId, {
        status: { Type: "pending" },
        tries,
        lastModified: BigInt(Date.now()),
        errors: [],
      }),
      ctx.scheduler.cancel(row.status.CleanupId),
    ]);

    return { ok: true, code: "Ok" } as const;
  },
});

export const fail = mutation({
  args: {
    requestId: v.id(requestsId),
    authedId: requestInProgress.by,
    reason: requestFailed.reason,
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotInProgress"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotTakenByYou"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
      reason: v.string(),
      requesterId: v.id(authedId),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return {
        ok: false,
        code: "RequestNotFound",
      } as const;
    }

    if (row.status.Type !== "inProgress") {
      return {
        ok: false,
        code: "RequestNotInProgress",
      } as const;
    }

    if (row.status.by !== args.authedId && row.requester !== args.authedId) {
      return {
        ok: false,
        code: "RequestNotTakenByYou",
      } as const;
    }

    await Promise.all([
      ctx.db.patch(args.requestId, {
        status: {
          Type: "failed",
          at: BigInt(Date.now()),
          by: args.authedId,
          reason: args.reason,
        },
        lastModified: BigInt(Date.now()),
      }),

      ctx.scheduler.cancel(row.status.CleanupId),
    ]);

    return {
      ok: true,
      code: "Ok",
      reason: args.reason,
      requesterId: row.requester,
    } as const;
  },
});

export const finish = mutation({
  args: {
    requestId: v.id(requestsId),
    requesterId: v.id(authedId),
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotInProgress"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotSubmittedByYou"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return {
        ok: false,
        code: "RequestNotFound",
      } as const;
    }

    if (row.status.Type !== "inProgress") {
      return {
        ok: false,
        code: "RequestNotInProgress",
      } as const;
    }

    if (row.requester !== args.requesterId) {
      return {
        ok: false,
        code: "RequestNotSubmittedByYou",
      } as const;
    }

    await ctx.db.patch(args.requestId, {
      status: {
        Type: "done",
        at: BigInt(Date.now()),
        by: row.status.by,
      },
      lastModified: BigInt(Date.now()),
    });

    if (row.status.CleanupId) {
      await ctx.scheduler.cancel(row.status.CleanupId);
    }

    return {
      ok: true,
      code: "Ok",
    } as const;
  },
});

export const clearRefusals = mutation({
  args: {
    requestId: v.id(requestsId),
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return { ok: false, code: "RequestNotFound" } as const;
    }

    await ctx.db.patch(args.requestId, {
      refusedBy: [],
      tries: 0n,
      lastModified: BigInt(Date.now()),
    });

    return { ok: true, code: "Ok" } as const;
  },
});

const requestStatusType = v.union(
  v.literal("pending"),
  v.literal("inProgress"),
  v.literal("done"),
  v.literal("failed"),
);

export const getByStatus = query({
  args: {
    statusType: requestStatusType,
    limit: v.optional(v.int64()),
    cursor: v.optional(v.string()),
  },
  returns: v.object({
    page: v.array(v.object(requestDataReturn)),
    isDone: v.boolean(),
    continueCursor: v.string(),
  }),
  handler: async (ctx, args) => {
    const numItems =
      args.limit !== undefined && args.limit > 0n ? Number(args.limit) : 100;

    const result = await ctx.db
      .query(requestsId)
      .withIndex("by_status_creation", (q) =>
        q.eq("status.Type", args.statusType),
      )
      .order("desc")
      .paginate({
        numItems,
        cursor: args.cursor ?? null,
      });

    return {
      page: result.page.map((row) => ({
        requestId: row._id,
        requester: row.requester,
        info: row.info,
        metadata: row.metadata,
        status: row.status,
        errors: row.errors,
        refusedBy: row.refusedBy ?? [],
        idempotencyKey: row.idempotencyKey,
        lastModified: row.lastModified,
        createdAt: row._creationTime,
        orderedBy: row.orderedBy,
        orderedIn: row.orderedIn,
      })),
      isDone: result.isDone,
      continueCursor: result.continueCursor,
    };
  },
});

/// Latest `lastModified` across all requests, or null if there are none. Used
/// by the admin live-stream as a "data changed" ping — the value advances
/// monotonically whenever any request is mutated.
export const getLatestChange = query({
  args: {},
  returns: v.object({
    lastModified: v.union(v.int64(), v.null()),
  }),
  handler: async (ctx) => {
    const row = await ctx.db
      .query(requestsId)
      .withIndex("by_last_modified")
      .order("desc")
      .first();
    return { lastModified: row ? row.lastModified : null };
  },
});

type Platform = "telegram" | "discord";
type AccountRef = { platform: Platform; id: string };

interface ParsedStatusMessage {
  channel_id?: string;
  chat_id?: string;
  author_id?: string;
}

function parseStatusMessage(
  row: Doc<"downloader_hub_requests">,
): ParsedStatusMessage | null {
  const blob = row.metadata?.status_message;
  if (!blob) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(blob);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const raw = parsed as Record<string, unknown>;
  const str = (v: unknown): string | undefined =>
    v === null || v === undefined ? undefined : String(v);
  const channel_id = str(raw.channel_id);
  const chat_id = str(raw.chat_id);
  const author_id = str(raw.author_id);
  const out: ParsedStatusMessage = {};
  if (channel_id) out.channel_id = channel_id;
  if (chat_id) out.chat_id = chat_id;
  if (author_id) out.author_id = author_id;
  return out;
}

function parseIdempotencyKey(
  key: string,
): { platform: Platform; placeId: string } | null {
  const discord = /^discord-(\d+)-\d+-\d+$/.exec(key);
  if (discord) return { platform: "discord", placeId: discord[1] };
  const tg = /^tg-(-?\d+)-\d+-\d+$/.exec(key);
  if (tg) return { platform: "telegram", placeId: tg[1] };
  return null;
}

function refsEqual(a: AccountRef | undefined, b: AccountRef): boolean {
  return !!a && a.platform === b.platform && a.id === b.id;
}

export const backfillOrderedRefs = internalMutation({
  args: {
    cursor: v.optional(v.string()),
    processed: v.optional(v.number()),
    patched: v.optional(v.number()),
    skipped: v.optional(v.number()),
  },
  returns: v.object({
    done: v.boolean(),
    processed: v.number(),
    patched: v.number(),
    skipped: v.number(),
  }),
  handler: async (ctx, args) => {
    const PAGE = 100;
    const processed = args.processed ?? 0;
    const patched = args.patched ?? 0;
    const skipped = args.skipped ?? 0;

    const result = await ctx.db.query(requestsId).paginate({
      numItems: PAGE,
      cursor: args.cursor ?? null,
    });
    const { page, continueCursor, isDone } = result;

    let pagePatched = 0;
    let pageSkipped = 0;
    const now = BigInt(Date.now());

    for (const row of page) {
      const hasBy = !!row.orderedBy;
      const hasIn = !!row.orderedIn;
      if (hasBy && hasIn) {
        pageSkipped += 1;
        continue;
      }

      const parsed = row.idempotencyKey
        ? parseIdempotencyKey(row.idempotencyKey)
        : null;
      const sm = parseStatusMessage(row);

      let platform: Platform | null = null;
      let placeId: string | null = null;
      let userId: string | null = null;

      if (parsed) {
        platform = parsed.platform;
        placeId = parsed.placeId;
      }
      if (sm) {
        if (!platform) {
          if (sm.channel_id) platform = "discord";
          else if (sm.chat_id) platform = "telegram";
        }
        if (!placeId) {
          placeId = sm.channel_id ?? sm.chat_id ?? null;
        }
        if (sm.author_id) userId = sm.author_id;
        else if (
          platform === "telegram" &&
          sm.chat_id &&
          sm.chat_id.startsWith("-") === false
        ) {
          userId = sm.chat_id;
        }
      }

      const orderedIn: AccountRef | null =
        platform && placeId ? { platform, id: placeId } : null;
      const orderedBy: AccountRef | null =
        platform && userId ? { platform, id: userId } : null;

      const patchBy =
        !hasBy && orderedBy && !refsEqual(row.orderedBy, orderedBy)
          ? orderedBy
          : null;
      const patchIn =
        !hasIn && orderedIn && !refsEqual(row.orderedIn, orderedIn)
          ? orderedIn
          : null;

      if (!patchBy && !patchIn) {
        pageSkipped += 1;
        continue;
      }

      await ctx.db.patch(row._id, {
        ...(patchBy ? { orderedBy: patchBy } : {}),
        ...(patchIn ? { orderedIn: patchIn } : {}),
        lastModified: now,
      });
      pagePatched += 1;
    }

    const totals = {
      processed: processed + page.length,
      patched: patched + pagePatched,
      skipped: skipped + pageSkipped,
    };

    if (isDone) return { done: true, ...totals };

    await ctx.scheduler.runAfter(0, internal.requests.backfillOrderedRefs, {
      cursor: continueCursor,
      ...totals,
    });
    return { done: false, ...totals };
  },
});

export const startBackfillOrderedRefs = mutation({
  args: {},
  returns: v.object({
    started: v.boolean(),
  }),
  handler: async (ctx) => {
    await ctx.scheduler.runAfter(0, internal.requests.backfillOrderedRefs, {});
    return { started: true };
  },
});

export const counts = query({
  args: {},
  returns: v.object({
    pending: v.int64(),
    inProgress: v.int64(),
    done: v.int64(),
    failed: v.int64(),
  }),
  handler: async (ctx) => {
    const statuses = ["pending", "inProgress", "done", "failed"] as const;
    const entries = await Promise.all(
      statuses.map(async (s) => {
        const count = await requestCountsAggregate.count(ctx, {
          namespace: s,
        });
        return [s, BigInt(count)] as const;
      }),
    );
    return Object.fromEntries(entries) as {
      pending: bigint;
      inProgress: bigint;
      done: bigint;
      failed: bigint;
    };
  },
});

export const retry = mutation({
  args: {
    requestId: v.id(requestsId),
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotRetryable"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return { ok: false, code: "RequestNotFound" } as const;
    }

    if (row.status.Type !== "failed" && row.status.Type !== "done") {
      return { ok: false, code: "RequestNotRetryable" } as const;
    }

    await ctx.db.patch(args.requestId, {
      status: { Type: "pending" },
      tries: 0n,
      errors: [],
      refusedBy: [],
      lastModified: BigInt(Date.now()),
    });

    return { ok: true, code: "Ok" } as const;
  },
});

export const cancel = mutation({
  args: {
    requestId: v.id(requestsId),
    by: v.id(authedId),
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return { ok: false, code: "RequestNotFound" } as const;
    }

    if (row.status.Type === "pending") {
      await ctx.db.patch(args.requestId, {
        status: {
          Type: "failed",
          at: BigInt(Date.now()),
          by: args.by,
          reason: "Cancelled by admin",
        },
        lastModified: BigInt(Date.now()),
      });
      return { ok: true, code: "Ok" } as const;
    }

    if (row.status.Type === "inProgress") {
      const ops: Promise<unknown>[] = [
        ctx.db.patch(args.requestId, {
          status: {
            Type: "failed",
            at: BigInt(Date.now()),
            by: args.by,
            reason: "Cancelled by admin",
          },
          lastModified: BigInt(Date.now()),
        }),
      ];
      if (row.status.CleanupId) {
        ops.push(ctx.scheduler.cancel(row.status.CleanupId));
      }
      await Promise.all(ops);
      return { ok: true, code: "Ok" } as const;
    }

    return { ok: true, code: "Ok" } as const;
  },
});

export const remove = mutation({
  args: {
    requestId: v.id(requestsId),
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("RequestNotFound"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.requestId);

    if (!row) {
      return { ok: false, code: "RequestNotFound" } as const;
    }

    const ops: Promise<unknown>[] = [ctx.db.delete(args.requestId)];

    if (row.status.Type === "inProgress" && row.status.CleanupId) {
      ops.push(ctx.scheduler.cancel(row.status.CleanupId));
    }

    await Promise.all(ops);

    return { ok: true, code: "Ok" } as const;
  },
});

/**
 * Backfill the `requestCounts` aggregate from existing `requests` rows.
 * Run ONCE after deploying the aggregate component, then never again:
 *
 *   npx convex run requests:backfillCounts '{}'
 *
 * Assumes the aggregate is empty when invoked (it is, immediately after the
 * component is first deployed). The trigger in `lib/triggers.ts` keeps it in
 * sync for all subsequent writes. Paginates + reschedules itself so it works
 * on large tables without exceeding mutation limits.
 */
export const backfillCounts = internalMutation({
  args: {
    cursor: v.optional(v.string()),
    processed: v.optional(v.number()),
  },
  returns: v.object({
    done: v.boolean(),
    processed: v.number(),
  }),
  handler: async (ctx, args) => {
    const PAGE = 100;
    const processed = args.processed ?? 0;

    const result = await ctx.db.query(requestsId).paginate({
      numItems: PAGE,
      cursor: args.cursor ?? null,
    });
    const { page, continueCursor, isDone } = result;

    await Promise.all(
      page.map((doc) => requestCountsAggregate.insert(ctx, doc)),
    );

    const totalProcessed = processed + page.length;
    if (isDone) {
      return { done: true, processed: totalProcessed };
    }

    // Recurse via the scheduler to stay within mutation time/transaction limits.
    await ctx.scheduler.runAfter(0, internal.requests.backfillCounts, {
      cursor: continueCursor,
      processed: totalProcessed,
    });
    return { done: false, processed: totalProcessed };
  },
});
