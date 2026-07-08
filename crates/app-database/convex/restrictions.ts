import { v } from "convex/values";
import { mutation, query } from "./_generated/server";
import { pick } from "convex-helpers";
import {
  accountPlaceRef,
  accountUserRef,
  restriction,
  restrictionId,
} from "./schema";

const restrictionReturn = {
  id: v.id(restrictionId),
  ...pick(restriction, ["user", "place", "rule"]),
};

function mapRow(row: any) {
  return {
    id: row._id,
    user: row.user,
    place: row.place,
    rule: row.rule,
  };
}

export const listBans = query({
  args: {},
  returns: v.array(v.object(restrictionReturn)),
  handler: async (ctx) => {
    const rows = await ctx.db
      .query(restrictionId)
      .withIndex("by_rule_type", (q) => q.eq("rule.Type", "ban"))
      .collect();
    return rows.map(mapRow);
  },
});

export const listLimits = query({
  args: {},
  returns: v.array(v.object(restrictionReturn)),
  handler: async (ctx) => {
    const rows = await ctx.db
      .query(restrictionId)
      .withIndex("by_rule_type", (q) => q.eq("rule.Type", "limit"))
      .collect();
    return rows.map(mapRow);
  },
});

export const getAll = query({
  args: {},
  returns: v.array(v.object(restrictionReturn)),
  handler: async (ctx) => {
    const rows = await ctx.db.query(restrictionId).collect();
    return rows.map(mapRow);
  },
});

export const create = mutation({
  args: {
    user: v.optional(accountUserRef),
    place: v.optional(accountPlaceRef),
    rule: restriction.rule,
  },
  returns: v.object({
    id: v.id(restrictionId),
  }),
  handler: async (ctx, args) => {
    const id = await ctx.db.insert(restrictionId, {
      user: args.user,
      place: args.place,
      rule: args.rule,
    });
    return { id };
  },
});

export const replace = mutation({
  args: {
    id: v.id(restrictionId),
    user: v.optional(accountUserRef),
    place: v.optional(accountPlaceRef),
    rule: restriction.rule,
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
    await ctx.db.patch(args.id, {
      user: args.user,
      place: args.place,
      rule: args.rule,
    });
    return { ok: true, code: "Ok" } as const;
  },
});

export const remove = mutation({
  args: {
    id: v.id(restrictionId),
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
