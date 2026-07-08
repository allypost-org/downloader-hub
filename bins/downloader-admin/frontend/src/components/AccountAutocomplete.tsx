import { useEffect, useMemo, useRef, useState } from "react";
import type {
  AccountPlaceInfo,
  AccountPlatform,
  AccountUserInfo,
} from "@/lib/api";
import { cn } from "@/lib/utils";

export interface AccountAutocompleteProps {
  platform: AccountPlatform;
  users?: AccountUserInfo[];
  places?: AccountPlaceInfo[];
  kind: "user" | "place";
  value: string;
  onChange: (id: string) => void;
  disabled?: boolean;
}

function userLabel(u: AccountUserInfo): string {
  return u.displayName ?? u.username ?? u.platformId;
}
function placeLabel(p: AccountPlaceInfo): string {
  return p.name ?? p.username ?? p.platformId;
}

export function AccountAutocomplete({
  platform,
  users,
  places,
  kind,
  value,
  onChange,
  disabled,
}: AccountAutocompleteProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState(value);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setQuery(value);
  }, [value]);

  useEffect(() => {
    if (!open) return;
    function onDocClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
        onChange(query.trim());
      }
    }
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [open, query, onChange]);

  const suggestions = useMemo(() => {
    const list = kind === "user" ? users : places;
    if (!list) return [];
    const q = query.trim().toLowerCase();
    return list
      .filter((x) => x.platform === platform)
      .filter((x) => {
        if (!q) return true;
        const label =
          kind === "user"
            ? userLabel(x as AccountUserInfo)
            : placeLabel(x as AccountPlaceInfo);
        return (
          x.platformId.toLowerCase().includes(q) ||
          label.toLowerCase().includes(q)
        );
      })
      .slice(0, 8);
  }, [kind, users, places, platform, query]);

  function pick(id: string) {
    onChange(id);
    setQuery(id);
    setOpen(false);
  }

  return (
    <div ref={ref} className="relative">
      <input
        type="text"
        value={query}
        disabled={disabled}
        placeholder={`${kind} platform id`}
        onFocus={() => setOpen(true)}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
          onChange(e.target.value.trim());
        }}
        className={cn(
          "h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
        )}
      />
      {open && suggestions.length > 0 && (
        <div className="absolute z-50 mt-1 max-h-60 w-full overflow-auto rounded-md border bg-card shadow-md">
          {suggestions.map((x) => {
            const label =
              kind === "user"
                ? userLabel(x as AccountUserInfo)
                : placeLabel(x as AccountPlaceInfo);
            return (
              <button
                key={`${x.platform}:${x.platformId}`}
                type="button"
                className="flex w-full items-center justify-between gap-2 px-3 py-1.5 text-left text-sm hover:bg-muted"
                onClick={() => pick(x.platformId)}
              >
                <span className="truncate">{label}</span>
                <code className="text-xs text-muted-foreground">
                  {x.platformId}
                </code>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
