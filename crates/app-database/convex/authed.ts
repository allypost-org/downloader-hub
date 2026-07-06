import { v } from "convex/values";
import { mutation, query } from "./_generated/server";
import { pick } from "convex-helpers";
import { authed, authedId } from "./schema";

export const getInfoByToken = query({
  args: {
    token: authed.token,
  },
  returns: v.union(
    v.object({
      Type: v.literal("authorized"),
      id: v.id(authedId),
      ...pick(authed, ["name", "for", "readonly", "onlyTagged", "expiresAt"]),
    }),
    v.object({
      Type: v.literal("not-authorized"),
      error: v.string(),
    }),
  ),
  handler: async (ctx, args) => {
    const info = await ctx.db
      .query(authedId)
      .withIndex("by_token", (q) => q.eq("token", args.token))
      .first();

    if (!info) {
      return {
        Type: "not-authorized",
        error: "unknown token",
      } as const;
    }

    if (info.expiresAt && info.expiresAt < Date.now()) {
      return {
        Type: "not-authorized",
        error: "token expired",
      } as const;
    }

    return {
      Type: "authorized",
      id: info._id,
      name: info.name,
      for: info.for,
      readonly: info.readonly,
      onlyTagged: info.onlyTagged,
      expiresAt: info.expiresAt,
    } as const;
  },
});
export const getInfoById = query({
  args: {
    id: v.id(authedId),
  },
  returns: v.union(
    v.object({
      Type: v.literal("authorized"),
      id: v.id(authedId),
      ...pick(authed, ["name", "for", "readonly", "onlyTagged", "expiresAt"]),
    }),
    v.object({
      Type: v.literal("not-authorized"),
      error: v.string(),
    }),
  ),
  handler: async (ctx, args) => {
    const info = await ctx.db.get(args.id);

    if (!info) {
      return {
        Type: "not-authorized",
        error: "unknown id",
      } as const;
    }

    if (info.expiresAt && info.expiresAt < Date.now()) {
      return {
        Type: "not-authorized",
        error: "token expired",
      } as const;
    }

    return {
      Type: "authorized",
      id: info._id,
      name: info.name,
      for: info.for,
      readonly: info.readonly,
      onlyTagged: info.onlyTagged,
      expiresAt: info.expiresAt,
    } as const;
  },
});

export const getAll = query({
  args: {},
  returns: v.array(
    v.object({
      id: v.id(authedId),
      expiresAt: v.optional(v.int64()),
    }),
  ),
  handler: async (ctx) => {
    const all = await ctx.db.query(authedId).collect();
    return all.map((doc) => ({ id: doc._id, expiresAt: doc.expiresAt }));
  },
});

const authedForRole = v.union(
  v.literal("worker"),
  v.literal("bot"),
  v.literal("admin"),
);

const authedFullReturn = {
  id: v.id(authedId),
  ...pick(authed, ["name", "for", "readonly", "onlyTagged", "expiresAt"]),
};

export const listFull = query({
  args: {},
  returns: v.array(v.object(authedFullReturn)),
  handler: async (ctx) => {
    const all = await ctx.db.query(authedId).collect();
    return all.map((doc) => ({
      id: doc._id,
      name: doc.name,
      for: doc.for,
      readonly: doc.readonly,
      onlyTagged: doc.onlyTagged,
      expiresAt: doc.expiresAt,
    }));
  },
});

function generateToken(): string {
  return Array.from({ length: 4 }, () =>
    crypto.randomUUID().replace(/-/g, ""),
  ).join("");
}

export const create = mutation({
  args: {
    name: authed.name,
    for: authedForRole,
    readonly: authed.readonly,
    onlyTagged: v.optional(v.array(v.string())),
    expiresAt: v.optional(v.int64()),
  },
  returns: v.object({
    id: v.id(authedId),
    token: v.string(),
  }),
  handler: async (ctx, args) => {
    const token = generateToken();
    const id = await ctx.db.insert(authedId, {
      name: args.name,
      token,
      readonly: args.readonly,
      for: args.for,
      onlyTagged: args.onlyTagged,
      expiresAt: args.expiresAt,
    });
    return { id, token };
  },
});

export const rotateToken = mutation({
  args: {
    id: v.id(authedId),
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("NotFound"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
      token: v.string(),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.id);
    if (!row) {
      return { ok: false, code: "NotFound" } as const;
    }
    const token = generateToken();
    await ctx.db.patch(args.id, { token });
    return { ok: true, code: "Ok", token } as const;
  },
});

export const revoke = mutation({
  args: {
    id: v.id(authedId),
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("NotFound"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.id);
    if (!row) {
      return { ok: false, code: "NotFound" } as const;
    }
    await ctx.db.patch(args.id, { expiresAt: BigInt(Date.now()) });
    return { ok: true, code: "Ok" } as const;
  },
});

export const remove = mutation({
  args: {
    id: v.id(authedId),
  },
  returns: v.union(
    v.object({
      ok: v.literal(false),
      code: v.literal("NotFound"),
    }),
    v.object({
      ok: v.literal(true),
      code: v.literal("Ok"),
    }),
  ),
  handler: async (ctx, args) => {
    const row = await ctx.db.get(args.id);
    if (!row) {
      return { ok: false, code: "NotFound" } as const;
    }
    await ctx.db.delete(args.id);
    return { ok: true, code: "Ok" } as const;
  },
});
