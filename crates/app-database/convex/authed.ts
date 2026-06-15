import { v } from "convex/values";
import { query } from "./_generated/server";
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
    })
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
    })
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
