"use client";

// ═══════════════════════════════════════════════════════════════
// SHINOBI — Auth Hook (Phase 19A)
// React context + hook for auth state management
// ═══════════════════════════════════════════════════════════════

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import {
  type AuthActor,
  getToken,
  getStoredUser,
  setToken,
  setStoredUser,
  clearToken,
  fetchMe,
  login as apiLogin,
  register as apiRegister,
} from "@/lib/auth";

// ── Context ──────────────────────────────────────────────────

interface AuthContextValue {
  /** Current authenticated user (null = anonymous). */
  user: AuthActor | null;
  /** True while hydrating auth state from storage/API. */
  loading: boolean;
  /** Login with email/password. */
  login: (email: string, password: string) => Promise<void>;
  /** Register a new account. */
  register: (data: {
    handle: string;
    display_name: string;
    email: string;
    password: string;
  }) => Promise<void>;
  /** Logout — clears token and user. */
  logout: () => void;
  /** Refresh user data from /me endpoint. */
  refresh: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

// ── Provider ─────────────────────────────────────────────────

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<AuthActor | null>(null);
  const [loading, setLoading] = useState(true);

  // Hydrate auth state on mount
  useEffect(() => {
    const token = getToken();
    if (!token) {
      setLoading(false);
      return;
    }

    // Instant display from cache
    const cached = getStoredUser();
    if (cached) setUser(cached);

    // Validate token against API
    fetchMe()
      .then((actor) => {
        setUser(actor);
        setStoredUser(actor);
      })
      .catch(() => {
        // Token expired or invalid
        clearToken();
        setUser(null);
      })
      .finally(() => setLoading(false));
  }, []);

  const login = useCallback(async (email: string, password: string) => {
    const result = await apiLogin({ email, password });
    setToken(result.token);
    setStoredUser(result.actor);
    setUser(result.actor);
  }, []);

  const register = useCallback(
    async (data: {
      handle: string;
      display_name: string;
      email: string;
      password: string;
    }) => {
      const result = await apiRegister(data);
      setToken(result.token);
      setStoredUser(result.actor);
      setUser(result.actor);
    },
    []
  );

  const logout = useCallback(() => {
    clearToken();
    setUser(null);
  }, []);

  const refresh = useCallback(async () => {
    try {
      const actor = await fetchMe();
      setUser(actor);
      setStoredUser(actor);
    } catch {
      clearToken();
      setUser(null);
    }
  }, []);

  return (
    <AuthContext.Provider value={{ user, loading, login, register, logout, refresh }}>
      {children}
    </AuthContext.Provider>
  );
}

// ── Hook ─────────────────────────────────────────────────────

/** Access auth state and actions from any component. */
export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return ctx;
}
