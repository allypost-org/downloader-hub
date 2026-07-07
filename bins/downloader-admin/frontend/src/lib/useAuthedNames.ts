import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";

/**
 * Resolves authed ids to display names. Prefers the WS-pushed `authed-names`
 * map (updated live via `/api/admin/stream`); falls back to fetching
 * `listAuthed` if no WS snapshot has arrived yet.
 */
export function useAuthedNames() {
  // Reactive view onto the WS-pushed name map. The `queryFn` is a no-op
  // placeholder — the data is populated externally by `useLiveStream` via
  // `qc.setQueryData(["authed-names"], ...)`.
  const live = useQuery<Record<string, string> | null>({
    queryKey: ["authed-names"],
    queryFn: () => null,
    staleTime: Infinity,
  });

  const fallback = useQuery({
    queryKey: ["authed"],
    queryFn: () => api.listAuthed(),
    staleTime: 60_000,
    enabled: !live.data,
  });

  const map = new Map<string, string>();
  if (live.data) {
    for (const [id, name] of Object.entries(live.data)) {
      map.set(id, name);
    }
  } else if (fallback.data) {
    for (const a of fallback.data) {
      map.set(a.id, a.name);
    }
  }

  function name(id: string | undefined | null): string | null {
    if (!id) return null;
    return map.get(id) ?? null;
  }

  return { name, loading: !live.data && fallback.isLoading };
}
