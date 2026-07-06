import { v } from "convex/values";
import { mutation as rawMutation, query } from "./_generated/server";
import { pick } from "convex-helpers";
import {
  accountPlace,
  accountPlaceId,
  accountUser,
  accountUserId,
} from "./schema";
import {
  customCtx,
  customMutation,
} from "convex-helpers/server/customFunctions";
import triggers from "./lib/triggers";

// Keep the trigger wrapping consistent with the other tables (see app-database/AGENTS.md).
const mutation = customMutation(rawMutation, customCtx(triggers.wrapDB));

const accountUserInput = v.object({
  ...pick(accountUser, [
    "platform",
    "platformId",
    "username",
    "displayName",
    "isBot",
  ]),
});

const accountPlaceInput = v.object({
  ...pick(accountPlace, [
    "platform",
    "platformId",
    "kind",
    "name",
    "username",
    "parentPlatformId",
  ]),
});

export const upsert = mutation({
  args: {
    users: v.array(accountUserInput),
    places: v.array(accountPlaceInput),
  },
  returns: v.object({
    users: v.int64(),
    places: v.int64(),
  }),
  handler: async (ctx, args) => {
    const now = BigInt(Date.now());

    let usersTouched = 0n;
    for (const user of args.users) {
      const existing = await ctx.db
        .query(accountUserId)
        .withIndex("by_platform_id", (q) =>
          q.eq("platform", user.platform).eq("platformId", user.platformId),
        )
        .first();

      if (existing) {
        await ctx.db.patch(existing._id, {
          username: user.username,
          displayName: user.displayName,
          isBot: user.isBot,
          lastSeen: now,
        });
      } else {
        await ctx.db.insert(accountUserId, {
          platform: user.platform,
          platformId: user.platformId,
          username: user.username,
          displayName: user.displayName,
          isBot: user.isBot,
          lastSeen: now,
        });
      }
      usersTouched += 1n;
    }

    let placesTouched = 0n;
    for (const place of args.places) {
      const existing = await ctx.db
        .query(accountPlaceId)
        .withIndex("by_platform_id", (q) =>
          q.eq("platform", place.platform).eq("platformId", place.platformId),
        )
        .first();

      if (existing) {
        await ctx.db.patch(existing._id, {
          kind: place.kind,
          name: place.name,
          username: place.username,
          parentPlatformId: place.parentPlatformId,
          lastSeen: now,
        });
      } else {
        await ctx.db.insert(accountPlaceId, {
          platform: place.platform,
          platformId: place.platformId,
          kind: place.kind,
          name: place.name,
          username: place.username,
          parentPlatformId: place.parentPlatformId,
          lastSeen: now,
        });
      }
      placesTouched += 1n;
    }

    return { users: usersTouched, places: placesTouched };
  },
});

const accountUserReturn = v.object({
  id: v.id(accountUserId),
  ...pick(accountUser, [
    "platform",
    "platformId",
    "username",
    "displayName",
    "isBot",
    "lastSeen",
  ]),
});

const accountPlaceReturn = v.object({
  id: v.id(accountPlaceId),
  ...pick(accountPlace, [
    "platform",
    "platformId",
    "kind",
    "name",
    "username",
    "parentPlatformId",
    "lastSeen",
  ]),
});

export const listUsers = query({
  args: {},
  returns: v.array(accountUserReturn),
  handler: async (ctx) => {
    const rows = await ctx.db.query(accountUserId).collect();
    return rows.map((row) => ({
      id: row._id,
      platform: row.platform,
      platformId: row.platformId,
      username: row.username,
      displayName: row.displayName,
      isBot: row.isBot,
      lastSeen: row.lastSeen,
    }));
  },
});

export const listPlaces = query({
  args: {},
  returns: v.array(accountPlaceReturn),
  handler: async (ctx) => {
    const rows = await ctx.db.query(accountPlaceId).collect();
    return rows.map((row) => ({
      id: row._id,
      platform: row.platform,
      platformId: row.platformId,
      kind: row.kind,
      name: row.name,
      username: row.username,
      parentPlatformId: row.parentPlatformId,
      lastSeen: row.lastSeen,
    }));
  },
});

// Minimal projection consumed by the admin live-stream to resolve
// `orderedBy`/`orderedIn` refs to display labels without re-fetching.
export const getAllForStream = query({
  args: {},
  returns: v.object({
    users: v.array(
      v.object({
        ...pick(accountUser, [
          "platform",
          "platformId",
          "username",
          "displayName",
        ]),
      }),
    ),
    places: v.array(
      v.object({
        ...pick(accountPlace, [
          "platform",
          "platformId",
          "kind",
          "name",
          "username",
          "parentPlatformId",
        ]),
      }),
    ),
  }),
  handler: async (ctx) => {
    const [users, places] = await Promise.all([
      ctx.db.query(accountUserId).collect(),
      ctx.db.query(accountPlaceId).collect(),
    ]);
    return {
      users: users.map((row) => ({
        platform: row.platform,
        platformId: row.platformId,
        username: row.username,
        displayName: row.displayName,
      })),
      places: places.map((row) => ({
        platform: row.platform,
        platformId: row.platformId,
        kind: row.kind,
        name: row.name,
        username: row.username,
        parentPlatformId: row.parentPlatformId,
      })),
    };
  },
});

const nullableString = v.union(v.string(), v.null());

export const updateUser = mutation({
  args: {
    id: v.id(accountUserId),
    username: v.optional(nullableString),
    displayName: v.optional(nullableString),
    isBot: v.optional(v.union(v.boolean(), v.null())),
  },
  returns: v.object({ ok: v.boolean() }),
  handler: async (ctx, args) => {
    const patch: Record<string, string | boolean | undefined> = {};
    if (args.username !== undefined) {
      patch.username = args.username ?? undefined;
    }
    if (args.displayName !== undefined) {
      patch.displayName = args.displayName ?? undefined;
    }
    if (args.isBot !== undefined) {
      patch.isBot = args.isBot ?? undefined;
    }
    await ctx.db.patch(args.id, patch);
    return { ok: true };
  },
});

export const updatePlace = mutation({
  args: {
    id: v.id(accountPlaceId),
    kind: v.optional(nullableString),
    name: v.optional(nullableString),
    username: v.optional(nullableString),
    parentPlatformId: v.optional(nullableString),
  },
  returns: v.object({ ok: v.boolean() }),
  handler: async (ctx, args) => {
    const patch: Record<string, string | undefined> = {};
    if (args.kind !== undefined) patch.kind = args.kind ?? undefined;
    if (args.name !== undefined) patch.name = args.name ?? undefined;
    if (args.username !== undefined)
      patch.username = args.username ?? undefined;
    if (args.parentPlatformId !== undefined) {
      patch.parentPlatformId = args.parentPlatformId ?? undefined;
    }
    await ctx.db.patch(args.id, patch);
    return { ok: true };
  },
});
