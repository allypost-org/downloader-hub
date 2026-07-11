export type RequestStatusType = "pending" | "inProgress" | "delivering" | "done" | "failed";

export type AccountPlatform = "telegram" | "discord";

export interface AccountRef {
  platform: AccountPlatform;
  id: string;
}

export interface EnvelopeError extends Error {
  status: number;
}

export interface RequestInfoResponse {
  requestId: string;
  info: unknown;
  metadata: Record<string, string>;
  status: {
    Type: RequestStatusType;
    since?: string;
    by?: string;
    message?: string;
    waitingForRequester?: boolean;
    filesData?: string;
    at?: string;
    reason?: string;
    deliveredBy?: string;
  };
  errors: string[];
  refusedBy: string[];
  requester: string;
  requesterName?: string;
  idempotencyKey?: string | null;
  lastModified: string;
  createdAt: number;
  orderedBy?: AccountRef | null;
  orderedIn?: AccountRef | null;
}

export interface RequestCounts {
  pending: string;
  inProgress: string;
  delivering: string;
  done: string;
  failed: string;
}

export interface RequestsByStatusPage {
  page: RequestInfoResponse[];
  isDone: boolean;
  continueCursor: string;
}

export interface MeResponse {
  id: string;
  name: string;
  forRole: string;
  readonly: boolean;
}

export interface AuthedFullInfo {
  id: string;
  name: string;
  for: "worker" | "bot" | "admin";
  readonly: boolean;
  onlyTagged: string[] | null;
  expiresAt: string | null;
}

export interface AuthedCreateInfo {
  id: string;
  token: string;
}

export interface ConnectionInfo {
  central: string;
  authed: string;
  role: string;
  capabilities: string | null;
  version: string | null;
  lastSeen: string;
}

export interface AdminSessionInfo {
  authedId: string;
  role: "worker" | "bot" | "admin";
  connectedAt: number;
  expiresAt: number | null;
}

export interface AdminParkedWorker {
  authedId: string;
  since: number;
}

export interface AccountUserInfo {
  id: string;
  platform: AccountPlatform;
  platformId: string;
  username: string | null;
  displayName: string | null;
  isBot: boolean | null;
  lastSeen: string;
}

export interface AccountPlaceInfo {
  id: string;
  platform: AccountPlatform;
  platformId: string;
  kind: string | null;
  name: string | null;
  username: string | null;
  parentPlatformId: string | null;
  lastSeen: string;
}

export type AccountUserPatchBody = {
  username?: string | null;
  displayName?: string | null;
  isBot?: boolean | null;
};

export type AccountPlacePatchBody = {
  kind?: string | null;
  name?: string | null;
  username?: string | null;
  parentPlatformId?: string | null;
};

export interface BanRule {
  Type: "ban";
  reason: string;
  endsAt: string | null;
}

export interface LimitRule {
  Type: "limit";
  count: string;
  timeframeMs: string;
}

export type RestrictionRule = BanRule | LimitRule;

export interface RestrictionInfo {
  id: string;
  user: AccountRef | null;
  place: AccountRef | null;
  rule: RestrictionRule;
}

export type CreateRestrictionBody = {
  user?: AccountRef;
  place?: AccountRef;
  rule:
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
};

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (method !== "GET") {
    headers["X-Downloader-Hub"] = btoa(String(Math.floor(Date.now() / 1000)));
  }
  const init: RequestInit = { method, headers, credentials: "same-origin" };
  if (body !== undefined) {
    init.body = JSON.stringify(body);
  }
  const res = await fetch(`/api/admin${path}`, init);
  const text = await res.text();
  let json: unknown = null;
  if (text) {
    try {
      json = JSON.parse(text);
    } catch {
      throw makeError(res.status, text);
    }
  }
  if (!res.ok) {
    const message =
      (json && typeof json === "object" && "error" in json
        ? String((json as Record<string, unknown>).error)
        : res.statusText) || `request failed (${res.status})`;
    throw makeError(res.status, message);
  }
  if (json && typeof json === "object" && "data" in json) {
    return (json as { data: T }).data;
  }
  return json as T;
}

function makeError(status: number, message: string): EnvelopeError {
  const err = new Error(message) as EnvelopeError;
  err.status = status;
  return err;
}

export const api = {
  login: (token: string) =>
    request<MeResponse>("POST", "/auth/login", { token }),
  logout: () => request<{ loggedOut: boolean }>("POST", "/auth/logout"),
  me: () => request<MeResponse>("GET", "/auth/me"),

  requestCounts: () => request<RequestCounts>("GET", "/requests/counts"),
  listRequests: (
    status: RequestStatusType,
    opts?: { limit?: number; cursor?: string },
  ) => {
    const params = new URLSearchParams({ status });
    if (opts?.limit) params.set("limit", String(opts.limit));
    if (opts?.cursor) params.set("cursor", opts.cursor);
    return request<RequestsByStatusPage>(
      "GET",
      `/requests?${params.toString()}`,
    );
  },
  listRequestsByUser: (
    platform: AccountPlatform,
    id: string,
    opts?: { status?: RequestStatusType; limit?: number; cursor?: string },
  ) => {
    const params = new URLSearchParams({ platform, id });
    if (opts?.status) params.set("status", opts.status);
    if (opts?.limit) params.set("limit", String(opts.limit));
    if (opts?.cursor) params.set("cursor", opts.cursor);
    return request<RequestsByStatusPage>(
      "GET",
      `/requests/by-user?${params.toString()}`,
    );
  },
  listRequestsByPlace: (
    platform: AccountPlatform,
    id: string,
    opts?: { status?: RequestStatusType; limit?: number; cursor?: string },
  ) => {
    const params = new URLSearchParams({ platform, id });
    if (opts?.status) params.set("status", opts.status);
    if (opts?.limit) params.set("limit", String(opts.limit));
    if (opts?.cursor) params.set("cursor", opts.cursor);
    return request<RequestsByStatusPage>(
      "GET",
      `/requests/by-place?${params.toString()}`,
    );
  },
  getRequest: (id: string) =>
    request<RequestInfoResponse>("GET", `/requests/${encodeURIComponent(id)}`),
  retryRequest: (id: string) =>
    request<{ retried: boolean }>("POST", `/requests/${encodeURIComponent(id)}/retry`),
  cancelRequest: (id: string) =>
    request<{ cancelled: boolean }>("POST", `/requests/${encodeURIComponent(id)}/cancel`),
  removeRequest: (id: string) =>
    request<{ removed: boolean }>("DELETE", `/requests/${encodeURIComponent(id)}`),
  clearRefusals: (id: string) =>
    request<unknown>("POST", `/requests/${encodeURIComponent(id)}/clear-refusals`),

  connections: () =>
    request<{ connections: ConnectionInfo[] }>("GET", "/connections"),
  metrics: () => fetch("/api/admin/metrics").then((r) => r.text()),

  listAuthed: () => request<AuthedFullInfo[]>("GET", "/authed"),
  createAuthed: (body: {
    name: string;
    for: "worker" | "bot" | "admin";
    readonly: boolean;
    onlyTagged?: string[];
    expiresAt?: number;
  }) => request<AuthedCreateInfo>("POST", "/authed", body),
  revokeAuthed: (id: string) =>
    request<{ revoked: boolean }>("POST", `/authed/${encodeURIComponent(id)}/revoke`),
  rotateAuthed: (id: string) =>
    request<{ token: string }>("POST", `/authed/${encodeURIComponent(id)}/rotate`),
  removeAuthed: (id: string) =>
    request<{ removed: boolean }>("DELETE", `/authed/${encodeURIComponent(id)}`),

  centralSessions: () =>
    request<AdminSessionInfo[]>("GET", "/central/sessions"),
  centralParkedWorkers: () =>
    request<AdminParkedWorker[]>("GET", "/central/parked-workers"),

  listAccountUsers: () =>
    request<AccountUserInfo[]>("GET", "/accounts/users"),
  listAccountPlaces: () =>
    request<AccountPlaceInfo[]>("GET", "/accounts/places"),
  updateAccountUser: (id: string, body: AccountUserPatchBody) =>
    request<{ ok: boolean }>(
      "PATCH",
      `/accounts/users/${encodeURIComponent(id)}`,
      body,
    ),
  updateAccountPlace: (id: string, body: AccountPlacePatchBody) =>
    request<{ ok: boolean }>(
      "PATCH",
      `/accounts/places/${encodeURIComponent(id)}`,
      body,
    ),

  backfillOrderedRefs: () =>
    request<{ started: boolean }>(
      "POST",
      "/requests/backfill-ordered-refs",
    ),

  refreshAccountUser: (id: string) =>
    request<{ requestId: string }>(
      "POST",
      `/accounts/users/${encodeURIComponent(id)}/refresh`,
    ),
  refreshAccountPlace: (id: string) =>
    request<{ requestId: string }>(
      "POST",
      `/accounts/places/${encodeURIComponent(id)}/refresh`,
    ),
  refreshStaleAccounts: () =>
    request<{ enqueued: number }>("POST", "/accounts/refresh-stale"),

  listRestrictions: (type: "ban" | "limit") =>
    request<RestrictionInfo[]>("GET", `/restrictions?type=${type}`),
  createRestriction: (body: CreateRestrictionBody) =>
    request<{ id: string }>("POST", "/restrictions", body),
  updateRestriction: (id: string, body: CreateRestrictionBody) =>
    request<{ updated: boolean }>(
      "PUT",
      `/restrictions/${encodeURIComponent(id)}`,
      body,
    ),
  removeRestriction: (id: string) =>
    request<{ removed: boolean }>(
      "DELETE",
      `/restrictions/${encodeURIComponent(id)}`,
    ),
};
