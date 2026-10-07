import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { api, type AccountRef } from "@/lib/api";

interface AccountNameMap {
  users: Record<string, string>;
  places: Record<string, string>;
}

const EMPTY: AccountNameMap = { users: {}, places: {} };

function keyOf(platform: string, platformId: string): string {
  return `${platform}:${platformId}`;
}

/** The `<platform>:<id>` key a ref is stored under in the name maps. */
export function accountRefKey(ref: AccountRef): string {
  return keyOf(ref.platform, ref.id);
}

/**
 * `true` when both refs point at the same account, which happens for private
 * chats where the bot stores the user and the place under one platform id.
 */
export function isSameRef(
  a: AccountRef | null | undefined,
  b: AccountRef | null | undefined,
): boolean {
  if (!a || !b) return false;
  return a.platform === b.platform && a.id === b.id;
}

/**
 * Resolves `orderedBy`/`orderedIn` refs to display labels. Prefers the
 * WS-pushed `account-names` map (live via `/api/admin/stream`); falls back to
 * `listAccountUsers`/`listAccountPlaces` when no snapshot has arrived yet, and
 * to the raw `<platform>:<id>` key when neither knows the ref.
 *
 * Label precedence mirrors the bin's live stream so both paths agree:
 * `displayName`/`name`, then `username`, then the platform id.
 */
export function useAccountNames() {
  // Reactive view onto the WS-pushed name map. The `queryFn` is a no-op
  // placeholder - the data is populated externally by `useLiveStream` via
  // `qc.setQueryData(["account-names"], ...)`.
  const live = useQuery<AccountNameMap | null>({
    queryKey: ["account-names"],
    queryFn: () => null,
    staleTime: Infinity,
  });

  // The stream only broadcasts changes, so a freshly loaded page sees no
  // snapshot until some account row changes; fetch once to fill the gap.
  // Same query keys the Accounts/Requests pages use, so this is a cache hit
  // whenever one of them already ran.
  const fallbackUsers = useQuery({
    queryKey: ["accounts", "users"],
    queryFn: api.listAccountUsers,
    staleTime: 30_000,
    enabled: !live.data,
  });
  const fallbackPlaces = useQuery({
    queryKey: ["accounts", "places"],
    queryFn: api.listAccountPlaces,
    staleTime: 30_000,
    enabled: !live.data,
  });

  const data = useMemo<AccountNameMap>(() => {
    if (live.data) return live.data;
    if (!fallbackUsers.data && !fallbackPlaces.data) return EMPTY;
    const users: Record<string, string> = {};
    for (const u of fallbackUsers.data ?? []) {
      users[keyOf(u.platform, u.platformId)] =
        u.displayName ?? u.username ?? u.platformId;
    }
    const places: Record<string, string> = {};
    for (const p of fallbackPlaces.data ?? []) {
      places[keyOf(p.platform, p.platformId)] =
        p.name ?? p.username ?? p.platformId;
    }
    return { users, places };
  }, [live.data, fallbackUsers.data, fallbackPlaces.data]);

  function userLabel(ref: AccountRef | null | undefined): string | null {
    if (!ref) return null;
    return data.users[accountRefKey(ref)] ?? null;
  }

  function placeLabel(ref: AccountRef | null | undefined): string | null {
    if (!ref) return null;
    return data.places[accountRefKey(ref)] ?? null;
  }

  function userLabelWithFallback(ref: AccountRef | null | undefined): string {
    return userLabel(ref) ?? (ref ? accountRefKey(ref) : "\u2014");
  }

  function placeLabelWithFallback(ref: AccountRef | null | undefined): string {
    return placeLabel(ref) ?? (ref ? accountRefKey(ref) : "\u2014");
  }

  return {
    userLabel,
    placeLabel,
    userLabelWithFallback,
    placeLabelWithFallback,
    loading: !live.data && fallbackUsers.isLoading && fallbackPlaces.isLoading,
  };
}