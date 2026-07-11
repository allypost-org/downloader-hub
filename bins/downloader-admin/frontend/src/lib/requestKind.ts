export type RequestKind = "downloadAndFix" | "refreshAccountInfo" | "unknown";

export function requestKindFromInfo(info: unknown): RequestKind {
  if (typeof info !== "object" || info === null) return "unknown";
  const record = info as Record<string, unknown>;
  if ("downloadAndFix" in record) return "downloadAndFix";
  if ("refreshAccountInfo" in record) return "refreshAccountInfo";
  return "unknown";
}

export function requestKindLabel(kind: RequestKind): string {
  switch (kind) {
    case "downloadAndFix":
      return "Download + fix";
    case "refreshAccountInfo":
      return "Account refresh";
    default:
      return "Unknown";
  }
}
