import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useAuthStore } from "@/stores/auth-store";

type NamesMap = Record<string, string>;

interface CountsEvent {
  type: "counts";
  pending: string;
  inProgress: string;
  delivering: string;
  done: string;
  failed: string;
}

interface AuthedNamesEvent {
  type: "authedNames";
  names: NamesMap;
}

interface AccountNamesEvent {
  type: "accountNames";
  users: NamesMap;
  places: NamesMap;
}

interface RequestsChangedEvent {
  type: "requestsChanged";
}

interface ResyncEvent {
  type: "resync";
}

type StreamEvent =
  | CountsEvent
  | AuthedNamesEvent
  | AccountNamesEvent
  | RequestsChangedEvent
  | ResyncEvent;

function parseEvent(raw: string): StreamEvent | null {
  let data: unknown;
  try {
    data = JSON.parse(raw);
  } catch {
    return null;
  }
  if (typeof data !== "object" || data === null || !("type" in data)) {
    return null;
  }
  switch ((data as StreamEvent).type) {
    case "counts":
    case "authedNames":
    case "accountNames":
    case "requestsChanged":
    case "resync":
      return data as StreamEvent;
    default:
      return null;
  }
}

function applyEvent(
  qc: ReturnType<typeof useQueryClient>,
  event: StreamEvent,
): void {
  switch (event.type) {
    case "counts":
      qc.setQueryData(["request-counts"], {
        pending: event.pending,
        inProgress: event.inProgress,
        delivering: event.delivering,
        done: event.done,
        failed: event.failed,
      });
      break;
    case "authedNames":
      qc.setQueryData(["authed-names"], event.names);
      break;
    case "accountNames":
      qc.setQueryData(["account-names"], {
        users: event.users,
        places: event.places,
      });
      break;
    case "requestsChanged":
      qc.invalidateQueries({ queryKey: ["requests"] });
      qc.invalidateQueries({ queryKey: ["request"] });
      break;
    case "resync":
      qc.invalidateQueries({ queryKey: ["request-counts"] });
      qc.invalidateQueries({ queryKey: ["authed-names"] });
      qc.invalidateQueries({ queryKey: ["account-names"] });
      qc.invalidateQueries({ queryKey: ["requests"] });
      qc.invalidateQueries({ queryKey: ["request"] });
      break;
    default:
      event satisfies never;
  }
}

const MIN_BACKOFF_MS = 2_000;
const MAX_BACKOFF_MS = 30_000;

function jitteredDelay(attempt: number): number {
  const base = Math.min(MAX_BACKOFF_MS, MIN_BACKOFF_MS * 2 ** attempt);
  return Math.floor(base * (0.5 + Math.random() * 0.5));
}

export function useLiveStream() {
  const qc = useQueryClient();
  const me = useAuthStore((s) => s.me);
  const clearAuth = useAuthStore((s) => s.clear);

  useEffect(() => {
    if (!me) return;

    let ws: WebSocket | null = null;
    let closedByUs = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let attempt = 0;
    let authChecked = false;

    function scheduleReconnect() {
      if (closedByUs) return;
      const delay = jitteredDelay(attempt);
      attempt += 1;
      reconnectTimer = setTimeout(connect, delay);
    }

    async function checkAuthAndMaybeClear() {
      if (authChecked || closedByUs) return;
      authChecked = true;
      try {
        const res = await fetch("/api/admin/auth/me", {
          credentials: "same-origin",
        });
        if (res.status === 401) {
          clearAuth();
          closedByUs = true;
        }
      } catch {
        // network blip - leave auth as-is, keep retrying the socket
      }
    }

    function connect() {
      const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      ws = new WebSocket(`${proto}//${window.location.host}/api/admin/stream`);

      ws.onopen = () => {
        attempt = 0;
        authChecked = false;
        // Catch up on anything missed while disconnected; the HTTP queries
        // are the source of truth, the stream only accelerates updates.
        applyEvent(qc, { type: "resync" });
      };

      ws.onmessage = (event) => {
        const parsed = parseEvent(event.data);
        if (parsed) applyEvent(qc, parsed);
      };

      ws.onclose = () => {
        if (closedByUs) return;
        void checkAuthAndMaybeClear();
        scheduleReconnect();
      };

      ws.onerror = () => {
        ws?.close();
      };
    }

    connect();

    return () => {
      closedByUs = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      ws?.close();
    };
  }, [qc, me, clearAuth]);
}
