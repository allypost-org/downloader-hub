import type {
  AccountPlatform,
  AccountRef,
  RestrictionInfo,
} from "@/lib/api";

export type RuleType = "ban" | "limit";
export type DurationMode = "indefinite" | "untilDate" | "forDuration";

export interface RuleFormState {
  ruleType: RuleType;
  reason: string;
  mode: DurationMode;
  endsAt: string;
  duration: string;
  count: string;
  timeframe: string;
}

export function emptyRuleForm(ruleType: RuleType = "ban"): RuleFormState {
  return {
    ruleType,
    reason: "",
    mode: "indefinite",
    endsAt: "",
    duration: "",
    count: "",
    timeframe: "",
  };
}

export function ruleFormFromRestriction(r: RestrictionInfo): RuleFormState {
  const base = emptyRuleForm(r.rule.Type);
  if (r.rule.Type === "ban") {
    base.reason = r.rule.reason;
    if (r.rule.endsAt) {
      const n = Number(r.rule.endsAt);
      if (Number.isFinite(n) && n > Date.now()) {
        base.mode = "untilDate";
        const d = new Date(n);
        const pad = (x: number) => String(x).padStart(2, "0");
        base.endsAt = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(
          d.getDate(),
        )}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
      }
    }
  } else {
    base.count = r.rule.count;
    base.timeframe = formatTimeframe(r.rule.timeframeMs).replace(
      /[^0-9a-z]/gi,
      "",
    );
  }
  return base;
}

export type BuiltRule =
  | {
      Type: "ban";
      reason: string;
      endsAt?: string;
      duration?: string;
    }
  | {
      Type: "limit";
      count: number;
      timeframe: string;
    };

export function buildRule(
  f: RuleFormState,
): { rule: BuiltRule; error: null } | { rule: null; error: string } {
  if (f.ruleType === "ban") {
    if (!f.reason.trim()) {
      return { rule: null, error: "Reason is required for a ban." };
    }
    return {
      rule: {
        Type: "ban",
        reason: f.reason.trim(),
        endsAt:
          f.mode === "untilDate" && f.endsAt
            ? new Date(f.endsAt).toISOString()
            : undefined,
        duration: f.mode === "forDuration" ? f.duration.trim() : undefined,
      },
      error: null,
    };
  }
  if (!f.count.trim() || !f.timeframe.trim()) {
    return { rule: null, error: "Count and timeframe are required for a limit." };
  }
  return {
    rule: {
      Type: "limit",
      count: Number(f.count),
      timeframe: f.timeframe.trim(),
    },
    error: null,
  };
}

export function formatExpiry(endsAt: string | null): string {
  if (!endsAt) return "indefinite";
  const n = Number(endsAt);
  if (!Number.isFinite(n)) return "indefinite";
  if (n < Date.now()) return "expired";
  return new Date(n).toLocaleString();
}

export function formatTimeframe(ms: string): string {
  const n = Number(ms);
  if (!Number.isFinite(n) || n <= 0) return "—";
  const s = n / 1000;
  if (s < 60) return `${s}s`;
  const m = s / 60;
  if (m < 60) return `${m}m`;
  const h = m / 60;
  if (h < 24) return `${h}h`;
  return `${h / 24}d`;
}

export function restrictionDetail(r: RestrictionInfo["rule"]): string {
  if (r.Type === "ban") {
    return `${r.reason} (${formatExpiry(r.endsAt)})`;
  }
  return `${r.count} per ${formatTimeframe(r.timeframeMs)}`;
}

/**
 * Whether a restriction row applies to a given account ref, per the scope rule:
 * `(row.user == null || row.user == ref) && (row.place == null || row.place == ref)`.
 * Pass the account as `kind: "user"` or `kind: "place"`.
 */
export function appliesToAccount(
  row: RestrictionInfo,
  kind: "user" | "place",
  ref: AccountRef,
): boolean {
  if (kind === "user") {
    const ru = row.user;
    return ru == null || (ru.platform === ref.platform && ru.id === ref.id);
  }
  const rp = row.place;
  return rp == null || (rp.platform === ref.platform && rp.id === ref.id);
}

export function refKey(ref: {
  platform: AccountPlatform;
  id: string;
}): string {
  return `${ref.platform}:${ref.id}`;
}
