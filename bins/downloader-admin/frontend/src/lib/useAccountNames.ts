import { useQuery } from "@tanstack/react-query";
import type { AccountRef } from "@/lib/api";

interface AccountNameMap {
  users: Record<string, string>;
  places: Record<string, string>;
}

const EMPTY: AccountNameMap = { users: {}, places: {} };

function refKey(ref: AccountRef): string {
  return `${ref.platform}:${ref.id}`;
}

/**
 * Resolves `orderedBy`/`orderedIn` refs to display labels. Prefers the
 * WS-pushed `account-names` map (live via `/api/admin/stream`); falls back to
 * the raw `<platform>:<id>` key when no metadata has arrived yet.
 */
export function useAccountNames() {
  const live = useQuery<AccountNameMap | null>({
    queryKey: ["account-names"],
    queryFn: () => null,
    staleTime: Infinity,
  });

  const data = live.data ?? EMPTY;

  function userLabel(ref: AccountRef | null | undefined): string | null {
    if (!ref) return null;
    return data.users[refKey(ref)] ?? null;
  }

  function placeLabel(ref: AccountRef | null | undefined): string | null {
    if (!ref) return null;
    return data.places[refKey(ref)] ?? null;
  }

  function userLabelWithFallback(ref: AccountRef | null | undefined): string {
    return userLabel(ref) ?? (ref ? refKey(ref) : "—");
  }

  function placeLabelWithFallback(ref: AccountRef | null | undefined): string {
    return placeLabel(ref) ?? (ref ? refKey(ref) : "—");
  }

  return {
    userLabel,
    placeLabel,
    userLabelWithFallback,
    placeLabelWithFallback,
    loading: !live.data,
  };
}
