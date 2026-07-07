import { create } from "zustand";
import { api, type MeResponse } from "@/lib/api";

interface AuthState {
  me: MeResponse | null;
  loading: boolean;
  error: string | null;
  fetchMe: () => Promise<MeResponse | null>;
  login: (token: string) => Promise<MeResponse>;
  logout: () => Promise<void>;
  clear: () => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  me: null,
  loading: false,
  error: null,
  fetchMe: async () => {
    set({ loading: true, error: null });
    try {
      const me = await api.me();
      set({ me, loading: false });
      return me;
    } catch (e) {
      set({ me: null, loading: false, error: errorMessage(e) });
      return null;
    }
  },
  login: async (token) => {
    set({ loading: true, error: null });
    const me = await api.login(token);
    set({ me, loading: false });
    return me;
  },
  logout: async () => {
    try {
      await api.logout();
    } finally {
      set({ me: null });
    }
  },
  clear: () => set({ me: null, error: null, loading: false }),
}));

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : "Unknown error";
}
