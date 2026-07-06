import { useEffect } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useAuthStore } from "@/stores/auth-store";

interface CountsMessage {
  type: "counts";
  data: {
    pending: string;
    inProgress: string;
    done: string;
    failed: string;
  };
}

interface RecentFailedMessage {
  type: "recentFailed";
  data: unknown[];
}

interface AuthedNamesMessage {
  type: "authedNames";
  data: Record<string, string>;
}

interface AccountNamesMessage {
  type: "accountNames";
  data: {
    users: Record<string, string>;
    places: Record<string, string>;
  };
}

interface RequestsChangedMessage {
  type: "requestsChanged";
  // latest `lastModified` (u64 ms epoch) or null when no requests exist
  data: string | number | null;
}

type StreamMessage =
  | CountsMessage
  | RecentFailedMessage
  | AuthedNamesMessage
  | AccountNamesMessage
  | RequestsChangedMessage;

function parseMessage(raw: string): StreamMessage | null {
  try {
    const msg = JSON.parse(raw) as StreamMessage;
    if (
      msg.type === "counts" ||
      msg.type === "recentFailed" ||
      msg.type === "authedNames" ||
      msg.type === "accountNames" ||
      msg.type === "requestsChanged"
    ) {
      return msg;
    }
    return null;
  } catch {
    return null;
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
        // network blip — leave auth as-is, keep retrying the socket
      }
    }

    function connect() {
      const proto = window.location.protocol === "https:" ? "wss:" : "ws:";
      ws = new WebSocket(`${proto}//${window.location.host}/api/admin/stream`);

      ws.onopen = () => {
        attempt = 0;
        authChecked = false;
      };

      ws.onmessage = (event) => {
        const msg = parseMessage(event.data);
        if (!msg) return;
        if (msg.type === "counts") {
          qc.setQueryData(["request-counts"], msg.data);
        } else if (msg.type === "recentFailed") {
          qc.setQueryData(["requests", "failed", "dashboard"], msg.data);
        } else if (msg.type === "authedNames") {
          qc.setQueryData(["authed-names"], msg.data);
        } else if (msg.type === "accountNames") {
          qc.setQueryData(["account-names"], msg.data);
        } else if (msg.type === "requestsChanged") {
          // Just a ping — invalidate paginated request queries so they refetch
          // via HTTP. The actual row data is not carried over the WS.
          qc.invalidateQueries({ queryKey: ["requests"] });
          qc.invalidateQueries({ queryKey: ["request"] });
        }
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
