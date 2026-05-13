/**
 * License Activation Page — First-launch flow.
 *
 * Shown when no valid license exists. User enters their license key
 * and creates the first local admin account.
 */

import { useState } from "react";
import { KeyRound, Shield, ArrowRight, Loader2, CheckCircle2, AlertCircle, UserPlus } from "lucide-react";
import { useAuthStore } from "@/lib/auth-store";

export function LicenseActivationPage() {
  const { activateLicense, loading, error, clearError } = useAuthStore();

  const [step, setStep] = useState<"key" | "admin">("key");
  const [licenseKey, setLicenseKey] = useState("");
  const [adminForm, setAdminForm] = useState({
    firstName: "",
    lastName: "",
    email: "",
    password: "",
    confirmPassword: "",
  });
  const [validationError, setValidationError] = useState("");

  function handleKeySubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!licenseKey.trim()) return;
    clearError();
    setStep("admin");
  }

  async function handleActivate(e: React.FormEvent) {
    e.preventDefault();
    if (!licenseKey.trim()) return;
    clearError();

    try {
      await activateLicense({
        license_key: licenseKey.trim(),
        admin_details: {
          first_name: adminForm.firstName,
          last_name: adminForm.lastName,
          email: adminForm.email,
          password: adminForm.password,
        }
      });
    } catch {
      // Error is set in the store
    }
  }

  return (
    <div className="flex min-h-full items-center justify-center bg-background p-4">
      <div className="w-full max-w-lg">
        {/* Logo / Brand */}
        <div className="mb-10 text-center">
          <div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-primary/20 to-accent/20 ring-1 ring-primary/30">
            <Shield className="h-8 w-8 text-primary" />
          </div>
          <h1 className="text-2xl font-bold tracking-tight">Wave Desktop</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            WiFi DensePose Sensing Platform
          </p>
        </div>

        {/* Activation Steps */}
        <div className="rounded-xl border border-border/60 bg-card/60 p-6 backdrop-blur">
          {step === "key" ? (
            <form onSubmit={handleKeySubmit} className="space-y-6">
              <div className="mb-4 flex items-center gap-3">
                <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10">
                  <KeyRound className="h-5 w-5 text-primary" />
                </div>
                <div>
                  <h2 className="font-semibold">License Activation</h2>
                  <p className="text-xs text-muted-foreground">Enter your license key to get started</p>
                </div>
              </div>

              <div>
                <label className="mb-1.5 block text-sm font-medium" htmlFor="license-key">
                  License Key
                </label>
                <input
                  id="license-key"
                  type="text"
                  value={licenseKey}
                  onChange={(e) => setLicenseKey(e.target.value)}
                  placeholder="WAVE-XXXX-XXXX-XXXX-XXXX"
                  className="h-11 w-full rounded-lg border border-input bg-background/70 px-4 font-mono text-sm tracking-widest placeholder:tracking-normal placeholder:font-sans focus:outline-none focus:ring-2 focus:ring-primary/40"
                  autoFocus
                />
              </div>

              <button
                type="submit"
                disabled={!licenseKey.trim()}
                className="flex h-11 w-full items-center justify-center gap-2 rounded-lg bg-primary font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
              >
                Continue to Admin Setup
                <ArrowRight className="h-4 w-4" />
              </button>
            </form>
          ) : (
            <form onSubmit={handleActivate} className="space-y-4">
              <div className="mb-4 flex items-center gap-3">
                <div className="flex h-10 w-10 items-center justify-center rounded-lg bg-primary/10">
                  <UserPlus className="h-5 w-5 text-primary" />
                </div>
                <div>
                  <h2 className="font-semibold">Administrator Setup</h2>
                  <p className="text-xs text-muted-foreground">Create the first system administrator account</p>
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="mb-1.5 block text-sm font-medium" htmlFor="admin-first">First Name</label>
                  <input
                    id="admin-first"
                    type="text"
                    required
                    value={adminForm.firstName}
                    onChange={(e) => setAdminForm(f => ({ ...f, firstName: e.target.value }))}
                    className="h-10 w-full rounded-lg border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-2 focus:ring-primary/40"
                  />
                </div>
                <div>
                  <label className="mb-1.5 block text-sm font-medium" htmlFor="admin-last">Last Name</label>
                  <input
                    id="admin-last"
                    type="text"
                    required
                    value={adminForm.lastName}
                    onChange={(e) => setAdminForm(f => ({ ...f, lastName: e.target.value }))}
                    className="h-10 w-full rounded-lg border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-2 focus:ring-primary/40"
                  />
                </div>
              </div>

              <div>
                <label className="mb-1.5 block text-sm font-medium" htmlFor="admin-email">Email Address</label>
                <input
                  id="admin-email"
                  type="email"
                  required
                  value={adminForm.email}
                  onChange={(e) => setAdminForm(f => ({ ...f, email: e.target.value }))}
                  className="h-10 w-full rounded-lg border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-2 focus:ring-primary/40"
                />
              </div>

              <div>
                <label className="mb-1.5 block text-sm font-medium" htmlFor="admin-pass">Password</label>
                <input
                  id="admin-pass"
                  type="password"
                  required
                  value={adminForm.password}
                  onChange={(e) => setAdminForm(f => ({ ...f, password: e.target.value }))}
                  className="h-10 w-full rounded-lg border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-2 focus:ring-primary/40"
                />
              </div>

              <div className="flex gap-3 pt-4">
                <button
                  type="button"
                  onClick={() => setStep("key")}
                  className="flex h-11 flex-1 items-center justify-center rounded-lg border border-border text-sm font-medium transition-colors hover:bg-secondary/70"
                >
                  Back
                </button>
                <button
                  type="submit"
                  disabled={loading}
                  className="flex h-11 flex-[2] items-center justify-center gap-2 rounded-lg bg-primary font-medium text-primary-foreground transition-colors hover:bg-primary/90 disabled:opacity-50"
                >
                  {loading ? (
                    <>
                      <Loader2 className="h-4 w-4 animate-spin" />
                      Activating System...
                    </>
                  ) : (
                    <>
                      Complete Setup
                      <CheckCircle2 className="h-4 w-4" />
                    </>
                  )}
                </button>
              </div>
            </form>
          )}

          {error && (
            <div className="mt-4 flex items-start gap-2 rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
              <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
              <span>{error}</span>
            </div>
          )}
        </div>

        {/* Footer */}
        <p className="mt-8 text-center text-xs text-muted-foreground">
          Wave Desktop v{__APP_VERSION__ ?? "0.3.0"} • Contact your vendor for license keys
        </p>
      </div>
    </div>
  );
}

declare const __APP_VERSION__: string | undefined;
