import type { GenericMutationCtx, GenericQueryCtx } from "convex/server";
import type { DataModel, Doc } from "../_generated/dataModel";
import { mutation, query } from "../_generated/server";
import { type PropertyValidators, v } from "convex/values";
import {
  type Customization,
  customMutation,
  customQuery,
} from "convex-helpers/server/customFunctions";
import { authedId } from "../schema";

export const authedQuery = customQuery(
  query,
  customAuthed({
    isMutation: false,
  })
);

export const authedMutation = customMutation(
  mutation,
  customAuthed({
    isMutation: true,
  })
);

type CustomAuthedExtraParams = {
  authIsOptional?: boolean;
  tags?: string[];
};
type CustomAuthedTopParams = {
  isMutation: boolean;
};
function customAuthed(params: CustomAuthedTopParams) {
  return customizationWrapper({
    args: {
      AuthToken: v.string(),
    },
    async input(ctx, args, extra: CustomAuthedExtraParams) {
      const token = await ctx.db
        .query(authedId)
        .withIndex("by_token", (q) => q.eq("token", args.AuthToken))
        .first();

      type Token = typeof token;

      checkAuth(token, {
        ...extra,
        ...params,
      });

      return {
        ctx: {
          authed: token as (typeof extra)["authIsOptional"] extends false
            ? Token
            : NonNullable<Token>,
        },
        args: {},
      };
    },
  });
}

function checkAuth(
  token: Doc<typeof authedId> | null,
  params: CustomAuthedExtraParams & CustomAuthedTopParams
) {
  if (params.authIsOptional) {
    return;
  }

  if (!token) {
    throw new Error("Permission denied. The provided token is invalid.");
  }

  if (params.isMutation && token.readonly) {
    throw new Error("Permission denied. The provided token is readonly.");
  }

  if (token.expiresAt && token.expiresAt < Date.now()) {
    throw new Error("Permission denied. The provided token has expired.");
  }

  if (token.onlyTagged) {
    const tokenRegexStr = token.onlyTagged
      .map((tag) => `(^${tag.replace("*", "(.*)")}$)`)
      .join("|");

    if (tokenRegexStr) {
      const tokenRegex = new RegExp(tokenRegexStr);

      const haveTags = params.tags?.some((tag) => tokenRegex.test(tag));
      if (!haveTags) {
        throw new Error(
          "Permission denied. The provided token does not have the required tags."
        );
      }
    }
  }
}

function customizationWrapper<
  CustomArgsValidator extends PropertyValidators,
  CustomCtx extends Record<string, any>,
  CustomMadeArgs extends Record<string, any>,
  ExtraArgs extends Record<string, any> = object,
>(
  x: Customization<
    GenericMutationCtx<DataModel> | GenericQueryCtx<DataModel>,
    CustomArgsValidator,
    CustomCtx,
    CustomMadeArgs,
    ExtraArgs
  >
) {
  return x;
}
