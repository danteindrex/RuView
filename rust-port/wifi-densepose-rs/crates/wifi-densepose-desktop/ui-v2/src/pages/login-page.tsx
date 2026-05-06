import { useState } from "react";
import { useAuthStore } from "@/lib/auth-store";

export function LoginPage() {
  const { login, loading, error, clearError, license } = useAuthStore();

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    clearError();

    if (!email.trim() || !password.trim()) return;

    try {
      await login(email.trim(), password);
    } catch {
      // Error displayed from store
    }
  }

  return (
    <div className="flex min-h-full items-center justify-center bg-background p-4">
      <div className="w-full max-w-md">
        {/* Logo / Brand */}
        <div className="mb-12 text-center">
          <div className="mx-auto mb-6 flex h-16 w-16 items-center justify-center rounded border border-primary/40 bg-primary/5">
            <span className="text-xl font-bold text-primary">WV</span>
          </div>
          <h1 className="text-2xl font-bold tracking-tight">Wave Desktop</h1>
          {license?.tenant_name && (
            <p className="mt-2 text-sm text-muted-foreground opacity-70">
              {license.tenant_name}
            </p>
          )}
        </div>

        {/* Login Form */}
        <form onSubmit={handleSubmit} className="space-y-6">
          <div className="rounded border border-border/60 bg-card/40 p-8 backdrop-blur shadow-2xl">
            <h2 className="mb-6 text-sm font-semibold text-muted-foreground border-b border-border/40 pb-2">Authentication Required</h2>
            <div className="space-y-5">
              <div>
                <label className="mb-2 block text-xs font-medium text-muted-foreground" htmlFor="login-email">
                  Email Address
                </label>
                <input
                  id="login-email"
                  type="email"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="name@example.com"
                  className="h-10 w-full rounded border border-input bg-background/50 px-4 text-sm focus:outline-none focus:ring-1 focus:ring-primary/60 transition-all"
                  autoFocus
                  autoComplete="email"
                />
              </div>

              <div>
                <label className="mb-2 block text-xs font-medium text-muted-foreground" htmlFor="login-password">
                  Password
                </label>
                <input
                  id="login-password"
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className="h-10 w-full rounded border border-input bg-background/50 px-4 text-sm focus:outline-none focus:ring-1 focus:ring-primary/60 transition-all"
                  autoComplete="current-password"
                />
              </div>
            </div>

            {error && (
              <div className="mt-6 rounded border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
                Error: {error}
              </div>
            )}
          </div>

          <button
            type="submit"
            disabled={loading || !email.trim() || !password.trim()}
            className="h-12 w-full rounded bg-primary text-sm font-medium text-primary-foreground transition-all hover:bg-primary/90 disabled:opacity-50"
          >
            {loading ? "Signing in..." : "Sign In"}
          </button>
        </form>

        {/* Footer */}
        <div className="mt-12 text-center text-xs text-muted-foreground opacity-60 space-y-2">
          {license?.tier && (
            <p>
              License: {license.tier}
              {license.expires_at && (
                <> • Expires: {new Date(license.expires_at).toLocaleDateString()}</>
              )}
            </p>
          )}
          <p>Wave Desktop v{__APP_VERSION__ ?? "0.3.0"}</p>
        </div>
      </div>
    </div>
  );
}

declare const __APP_VERSION__: string | undefined;
