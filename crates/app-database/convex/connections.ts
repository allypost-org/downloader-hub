import { query, mutation, internalMutation } from "./_generated/server";
import type { QueryCtx } from "./_generated/server";
import type { Doc, Id } from "./_generated/dataModel";
import { v } from "convex/values";
import { authedId, connections, connectionsId } from "./schema";

const CONNECTION_TTL_MS = 5 * 60 * 1000;

async function find(
  ctx: QueryCtx,
  central: string,
  authed: Id<typeof authedId>,
): Promise<Doc<typeof connectionsId> | null> {
  return await ctx.db
    .query(connectionsId)
    .withIndex("by_central_authed", (q) =>
      q.eq("central", central).eq("authed", authed),
    )
    .unique();
}

export const upsert = mutation({
  args: {
    central: connections.central,
    authed: connections.authed,
    role: connections.role,
    capabilities: connections.capabilities,
    version: connections.version,
  },
  returns: v.null(),
  handler: async (ctx, args) => {
    const now = BigInt(Date.now());
    const existing = await find(ctx, args.central, args.authed);
    if (existing) {
      await ctx.db.patch(existing._id, {
        role: args.role,
        capabilities: args.capabilities,
        version: args.version,
        lastSeen: now,
      });
    } else {
      await ctx.db.insert(connectionsId, {
        central: args.central,
        authed: args.authed,
        role: args.role,
        capabilities: args.capabilities,
        version: args.version,
        lastSeen: now,
      });
    }
    return null;
  },
});

export const heartbeat = mutation({
  args: {
    central: connections.central,
    authed: connections.authed,
  },
  returns: v.null(),
  handler: async (ctx, args) => {
    const existing = await find(ctx, args.central, args.authed);
    if (existing) {
      await ctx.db.patch(existing._id, { lastSeen: BigInt(Date.now()) });
    }
    return null;
  },
});

export const remove = mutation({
  args: {
    central: connections.central,
    authed: connections.authed,
  },
  returns: v.null(),
  handler: async (ctx, args) => {
    const existing = await find(ctx, args.central, args.authed);
    if (existing) {
      await ctx.db.delete(existing._id);
    }
    return null;
  },
});

export const list = query({
  args: {},
  returns: v.array(
    v.object({
      central: connections.central,
      authed: connections.authed,
      role: connections.role,
      capabilities: connections.capabilities,
      version: connections.version,
      lastSeen: connections.lastSeen,
    }),
  ),
  handler: async (ctx) => {
    const rows = await ctx.db.query(connectionsId).collect();
    return rows.map((r) => ({
      central: r.central,
      authed: r.authed,
      role: r.role,
      capabilities: r.capabilities,
      version: r.version,
      lastSeen: r.lastSeen,
    }));
  },
});

export const cleanup = internalMutation({
  args: {},
  returns: v.null(),
  handler: async (ctx) => {
    const cutoff = BigInt(Date.now() - CONNECTION_TTL_MS);
    const stale = await ctx.db
      .query(connectionsId)
      .withIndex("by_last_seen", (q) => q.lt("lastSeen", cutoff))
      .collect();
    await Promise.all(stale.map((r) => ctx.db.delete(r._id)));
    return null;
  },
});
