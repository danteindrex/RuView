import { useState } from "react";
import { authApi, type CreateTenantRequest } from "@/lib/auth-api";
import { useAuthStore } from "@/lib/auth-store";

interface CreateTenantDialogProps {
  onClose: () => void;
  onSuccess: () => void;
}

export function CreateTenantDialog({ onClose, onSuccess }: CreateTenantDialogProps) {
  const { accessToken } = useAuthStore();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [form, setForm] = useState<CreateTenantRequest>({
    name: "",
    industry: "Technology",
    domain: "",
    email: "",
    currency_code: "USD",
    description: "",
  });

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!accessToken || !form.name) return;

    setLoading(true);
    setError("");
    try {
      await authApi.createTenant(accessToken, form);
      onSuccess();
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="w-full max-w-md rounded-xl border border-border/60 bg-card p-6 shadow-2xl">
        <h3 className="mb-6 text-lg font-semibold tracking-tight">Create New Tenant</h3>
        
        {error && (
          <div className="mb-4 rounded border border-destructive/20 bg-destructive/10 p-3 text-xs text-destructive">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground" htmlFor="t-name">Organization Name</label>
            <input id="t-name" type="text" required value={form.name}
              onChange={(e) => setForm(f => ({ ...f, name: e.target.value }))}
              placeholder="e.g., Acme Corp"
              className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary" />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground" htmlFor="t-industry">Industry</label>
              <select id="t-industry" value={form.industry}
                onChange={(e) => setForm(f => ({ ...f, industry: e.target.value }))}
                className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary">
                <option value="Technology">Technology</option>
                <option value="Retail">Retail</option>
                <option value="Healthcare">Healthcare</option>
                <option value="Manufacturing">Manufacturing</option>
                <option value="Education">Education</option>
                <option value="Other">Other</option>
              </select>
            </div>
            <div className="space-y-1.5">
              <label className="text-xs font-medium text-muted-foreground" htmlFor="t-currency">Currency</label>
              <input id="t-currency" type="text" value={form.currency_code}
                onChange={(e) => setForm(f => ({ ...f, currency_code: e.target.value }))}
                className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary" />
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground" htmlFor="t-domain">Domain (Optional)</label>
            <input id="t-domain" type="text" value={form.domain}
              onChange={(e) => setForm(f => ({ ...f, domain: e.target.value }))}
              placeholder="acme.com"
              className="h-10 w-full rounded border border-input bg-background/70 px-3 text-sm focus:outline-none focus:ring-1 focus:ring-primary" />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-medium text-muted-foreground" htmlFor="t-desc">Description</label>
            <textarea id="t-desc" rows={2} value={form.description}
              onChange={(e) => setForm(f => ({ ...f, description: e.target.value }))}
              className="w-full rounded border border-input bg-background/70 px-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-primary resize-none" />
          </div>

          <div className="flex gap-3 pt-4">
            <button type="button" onClick={onClose} disabled={loading}
              className="flex h-10 flex-1 items-center justify-center rounded border border-border text-sm font-medium hover:bg-secondary/70">Cancel</button>
            <button type="submit" disabled={loading}
              className="flex h-10 flex-[2] items-center justify-center rounded bg-primary text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50">
              {loading ? "Creating..." : "Confirm Creation"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
