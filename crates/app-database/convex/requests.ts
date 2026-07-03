import { v } from "convex/values";
import { internalMutation, mutation, query } from "./_generated/server";
import { internal } from "./_generated/api";
import schema, {
  authedId,
  requestPending,
  requestFailed,
  requestInProgress,
  requests,
  requestsId,
} from "./schema";
import { Id } from "./_generated/dataModel";
import { mergedStream, stream } from "convex-helpers/server/stream";

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
    });

    return {
      requestId: id,
    };
  },
});

const requestDataReturn = {
  requestId: v.id(requestsId),
  info: requests.info,
  metadata: requests.metadata,
  status: requests.status,
  errors: requests.errors,
  refusedBy: requests.refusedBy,
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
      info: row.info,
      metadata: row.metadata,
      status: row.status,
      errors: row.errors,
      refusedBy: row.refusedBy ?? [],
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
      info: row.info,
      metadata: row.metadata,
      status: row.status,
      errors: row.errors,
      refusedBy: row.refusedBy ?? [],
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
      info: row.info,
      metadata: row.metadata,
      status: row.status,
      errors: row.errors,
      refusedBy: row.refusedBy ?? [],
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
      info: row.info,
      metadata: row.metadata,
      status,
      errors: row.errors,
      refusedBy: row.refusedBy ?? [],
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
