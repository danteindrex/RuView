/**
 * Auth Store — Zustand store for authentication state.
 *
 * Manages:
 * - License status (3-stage app flow: License → Login → Dashboard)
 * - User session (JWT tokens + user profile)
 * - Permission-based tab filtering
 * - Token refresh rotation
 */

import { create } from "zustand";
import { authApi, type AuthUser, type LicenseStatus } from "./auth-api";

// ─── App Stage ──────────────────────────────────────────────────────────────

/**
 * 3-stage startup flow:
 * 1. LICENSE — no valid license, show activation page
 * 2. LOGIN — licensed but no user session, show login page
 * 3. DASHBOARD — licensed + logged in, show the app
 */
export type AppStage = "loading" | "license" | "login" | "dashboard";

// ─── Store Shape ────────────────────────────────────────────────────────────

interface AuthState {
  // Startup
  stage: AppStage;
  loading: boolean;
  error: string | null;

  // License
  license: LicenseStatus | null;

  // Session
  accessToken: string | null;
  refreshToken: string | null;
  user: AuthUser | null;
  isSuperAdmin: boolean;

  // Actions
  initialize: () => Promise<void>;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  activateLicense: (request: {
    license_key: string;
    admin_details?: {
      first_name: string;
      last_name: string;
      email: string;
      password: string;
    };
  }) => Promise<void>;
  refreshSession: () => Promise<boolean>;
  setStage: (stage: AppStage) => void;
  clearError: () => void;
}

// ─── Persistent token storage ───────────────────────────────────────────────

const TOKEN_KEYS = {
  access: "wave-access-token",
  refresh: "wave-refresh-token",
} as const;

function saveTokens(access: string, refresh?: string) {
  localStorage.setItem(TOKEN_KEYS.access, access);
  if (refresh) localStorage.setItem(TOKEN_KEYS.refresh, refresh);
}

function loadTokens() {
  return {
    access: localStorage.getItem(TOKEN_KEYS.access),
    refresh: localStorage.getItem(TOKEN_KEYS.refresh),
  };
}

function clearTokens() {
  localStorage.removeItem(TOKEN_KEYS.access);
  localStorage.removeItem(TOKEN_KEYS.refresh);
}

let refreshInterval: number | null = null;

function setupRefreshTimer(refreshSession: () => Promise<boolean>) {
  if (refreshInterval) {
    window.clearInterval(refreshInterval);
  }
  // Refresh every 12 minutes
  refreshInterval = window.setInterval(async () => {
    await refreshSession();
  }, 12 * 60 * 1000);
}

function clearRefreshTimer() {
  if (refreshInterval) {
    window.clearInterval(refreshInterval);
    refreshInterval = null;
  }
}

// ─── Store ──────────────────────────────────────────────────────────────────

export const useAuthStore = create<AuthState>((set, get) => ({
  stage: "loading",
  loading: true,
  error: null,
  license: null,
  accessToken: null,
  refreshToken: null,
  user: null,
  isSuperAdmin: false,

  /**
   * Called on app mount. Determines the startup stage:
   * 1. Check license status
   * 2. If licensed, try to restore session from stored tokens
   * 3. Set appropriate stage
   */
  async initialize() {
    console.log("[AuthStore] Initializing...");
    set({ loading: true, error: null });
    try {
      const license = await authApi.licenseStatus();
      console.log("[AuthStore] License status:", license.is_licensed ? "Licensed" : "Unlicensed");
      set({ license });

      const stored = loadTokens();
      // Robust check for string "null"/"undefined" which can happen in some storage edge cases
      const hasToken = stored.access && stored.access !== "null" && stored.access !== "undefined";

      if (!hasToken) {
        console.log("[AuthStore] No session found. Routing to LOGIN.");
        set({ stage: "login", loading: false });
        return;
      }

      try {
        console.log("[AuthStore] Restoring session...");
        const user = await authApi.getCurrentUser(stored.access!);
        const isSuperAdmin = user.scope === "global";
        console.log("[AuthStore] Session restored. User scope:", user.scope);
        
        if (isSuperAdmin) {
          set({
            stage: "dashboard",
            accessToken: stored.access,
            refreshToken: stored.refresh,
            user,
            isSuperAdmin: true,
            loading: false,
          });
          setupRefreshTimer(get().refreshSession);
        } else if (!license.is_licensed) {
          console.log("[AuthStore] Regular user on unlicensed node. Routing to LICENSE.");
          set({ stage: "license", loading: false });
        } else {
          set({
            stage: "dashboard",
            accessToken: stored.access,
            refreshToken: stored.refresh,
            user,
            isSuperAdmin: false,
            loading: false,
          });
          setupRefreshTimer(get().refreshSession);
        }
      } catch (err) {
        clearTokens();
        clearRefreshTimer();
        set({ stage: "login", loading: false });
      }
    } catch (err) {
      console.error("Auth init failed:", err);
      set({ stage: "login", loading: false });
    }
  },

  /**
   * Dual-auth login: local or vendor (super admin).
   */
  async login(email, password) {
    set({ loading: true, error: null });
    try {
      const response = await authApi.login(email, password);
      const { access_token, refresh_token, user } = response;
      const isSuperAdmin = user.scope === "global";

      saveTokens(access_token, refresh_token);

      // Check license for non-super-admins
      if (!isSuperAdmin) {
        const license = await authApi.licenseStatus();
        if (!license.is_licensed) {
          set({
            stage: "license",
            license,
            accessToken: access_token,
            refreshToken: refresh_token ?? null,
            user,
            isSuperAdmin: false,
            loading: false,
          });
          return;
        }
      }

      set({
        stage: "dashboard",
        accessToken: access_token,
        refreshToken: refresh_token ?? null,
        user,
        isSuperAdmin,
        loading: false,
        error: null,
      });
      setupRefreshTimer(get().refreshSession);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      set({ loading: false, error: message });
      throw err;
    }
  },

  /**
   * Logout: revoke tokens + clear local state.
   */
  async logout() {
    const { accessToken } = get();
    try {
      if (accessToken) await authApi.logout(accessToken);
    } catch {
      // Ignore errors — we're clearing local state regardless
    }
    clearTokens();
    clearRefreshTimer();
    set({
      stage: "login",
      accessToken: null,
      refreshToken: null,
      user: null,
      isSuperAdmin: false,
      error: null,
    });
  },

  /**
   * Activate license + create first admin.
   */
  async activateLicense(request) {
    set({ loading: true, error: null });
    try {
      const license = await authApi.activateLicense(request);
      
      // If we are already a super admin, stay in the dashboard
      if (get().isSuperAdmin) {
        set({ license, loading: false, error: null });
      } else {
        // First-time setup: go to login to use the seeded admin
        set({
          stage: "login",
          license,
          loading: false,
          error: null,
        });
      }
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      set({ loading: false, error: message });
      throw err;
    }
  },

  /**
   * Refresh the access token using the refresh token.
   * Returns true if successful.
   */
  async refreshSession() {
    const { refreshToken: rt } = get();
    if (!rt) return false;

    try {
      const response = await authApi.refreshToken(rt);
      const user = await authApi.getCurrentUser(response.access_token);

      saveTokens(response.access_token, response.refresh_token);
      set({
        accessToken: response.access_token,
        refreshToken: response.refresh_token,
        user,
        isSuperAdmin: user.scope === "global",
      });
      setupRefreshTimer(get().refreshSession);
      return true;
    } catch {
      clearTokens();
      clearRefreshTimer();
      set({
        stage: "login",
        accessToken: null,
        refreshToken: null,
        user: null,
        isSuperAdmin: false,
      });
      return false;
    }
  },

  setStage: (stage: AppStage) => {
    set({ stage });
  },

  clearError() {
    set({ error: null });
  },
}));
