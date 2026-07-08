import { Input } from "@/components/ui/input";
import type { RuleFormState } from "@/lib/restrictions";

export function RestrictionRuleFields({
  f,
  set,
  readonly,
}: {
  f: RuleFormState;
  set: (patch: Partial<RuleFormState>) => void;
  readonly: boolean;
}) {
  return (
    <div className="space-y-3">
      <div className="flex gap-2">
        <label className="flex items-center gap-1">
          <input
            type="radio"
            checked={f.ruleType === "ban"}
            onChange={() => set({ ruleType: "ban" })}
            disabled={readonly}
          />
          <span className="text-sm">Ban</span>
        </label>
        <label className="flex items-center gap-1">
          <input
            type="radio"
            checked={f.ruleType === "limit"}
            onChange={() => set({ ruleType: "limit" })}
            disabled={readonly}
          />
          <span className="text-sm">Limit</span>
        </label>
      </div>

      {f.ruleType === "ban" ? (
        <>
          <label className="block space-y-1">
            <span className="text-xs text-muted-foreground">Reason</span>
            <Input
              value={f.reason}
              onChange={(e) => set({ reason: e.target.value })}
              placeholder="abuse"
              disabled={readonly}
              className="w-full max-w-md"
            />
          </label>
          <div className="space-y-2">
            <span className="text-xs text-muted-foreground">Duration</span>
            <div className="flex flex-wrap items-center gap-3">
              <label className="flex items-center gap-1">
                <input
                  type="radio"
                  checked={f.mode === "indefinite"}
                  onChange={() => set({ mode: "indefinite" })}
                  disabled={readonly}
                />
                <span className="text-sm">indefinite</span>
              </label>
              <label className="flex items-center gap-1">
                <input
                  type="radio"
                  checked={f.mode === "untilDate"}
                  onChange={() => set({ mode: "untilDate" })}
                  disabled={readonly}
                />
                <span className="text-sm">until date</span>
                <Input
                  type="datetime-local"
                  value={f.endsAt}
                  onChange={(e) => set({ endsAt: e.target.value })}
                  disabled={readonly || f.mode !== "untilDate"}
                  className="w-48"
                />
              </label>
              <label className="flex items-center gap-1">
                <input
                  type="radio"
                  checked={f.mode === "forDuration"}
                  onChange={() => set({ mode: "forDuration" })}
                  disabled={readonly}
                />
                <span className="text-sm">for</span>
                <Input
                  value={f.duration}
                  onChange={(e) => set({ duration: e.target.value })}
                  disabled={readonly || f.mode !== "forDuration"}
                  placeholder="1 hour"
                  className="w-32"
                />
              </label>
            </div>
          </div>
        </>
      ) : (
        <div className="flex flex-wrap items-end gap-4">
          <label className="space-y-1">
            <span className="text-xs text-muted-foreground">Count</span>
            <Input
              type="number"
              min="1"
              value={f.count}
              onChange={(e) => set({ count: e.target.value })}
              placeholder="5"
              disabled={readonly}
              className="w-28"
            />
          </label>
          <label className="space-y-1">
            <span className="text-xs text-muted-foreground">Timeframe</span>
            <Input
              value={f.timeframe}
              onChange={(e) => set({ timeframe: e.target.value })}
              placeholder="1 hour"
              disabled={readonly}
              className="w-40"
            />
          </label>
        </div>
      )}
    </div>
  );
}
