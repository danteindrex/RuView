import { useState } from "react";
import { 
  Dialog, 
  DialogContent, 
  DialogHeader, 
  DialogTitle, 
  DialogDescription,
  DialogFooter
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useAuthStore } from "@/lib/auth-store";

interface LicenseActivationDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function LicenseActivationDialog({ open, onOpenChange }: LicenseActivationDialogProps) {
  const { activateLicense, loading, error, clearError, initialize } = useAuthStore();
  const [licenseKey, setLicenseKey] = useState("");
  const [step, setStep] = useState<"key" | "admin">("key");
  const [adminForm, setAdminForm] = useState({
    firstName: "",
    lastName: "",
    email: "",
    password: "",
  });
  const [success, setSuccess] = useState(false);

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
      setSuccess(true);
      // Re-initialize to fetch new license status
      setTimeout(() => {
        initialize();
        onOpenChange(false);
      }, 1500);
    } catch {
      // Error is handled by store
    }
  }

  function handleKeyNext(e: React.FormEvent) {
    e.preventDefault();
    if (licenseKey.trim()) {
      setStep("admin");
    }
  }

  return (
    <Dialog open={open} onOpenChange={(val) => {
      if (!loading) {
        onOpenChange(val);
        if (!val) {
          setSuccess(false);
          setLicenseKey("");
          setStep("key");
          clearError();
        }
      }
    }}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <div className="mb-4">
            <span className="text-[10px] font-bold text-primary border border-primary/30 px-2 py-0.5 rounded uppercase tracking-wider">
              {step === "key" ? "License Auth" : "Admin Setup"}
            </span>
          </div>
          <DialogTitle className="text-xl font-semibold tracking-tight">
            {step === "key" ? "Activate System" : "Provision Administrator"}
          </DialogTitle>
          <DialogDescription className="text-sm text-muted-foreground">
            {step === "key" 
              ? "Enter your license key to authorize this installation." 
              : "Create the primary local administrator account."}
          </DialogDescription>
        </DialogHeader>

        {success ? (
          <div className="flex flex-col items-center justify-center py-10 text-center space-y-4">
            <div className="text-sm font-bold text-success ring-1 ring-success/30 px-4 py-2 rounded bg-success/5 uppercase tracking-wider">
              Activation Successful
            </div>
            <p className="text-sm text-muted-foreground">System authorization complete.</p>
          </div>
        ) : step === "key" ? (
          <form onSubmit={handleKeyNext} className="space-y-5 py-4">
            <div className="space-y-2">
              <Label htmlFor="modal-license-key" className="text-xs font-medium text-muted-foreground">License Key</Label>
              <Input
                id="modal-license-key"
                placeholder="WAVE-XXXX-XXXX-XXXX-XXXX"
                value={licenseKey}
                onChange={(e) => setLicenseKey(e.target.value)}
                className="font-mono text-sm h-10"
                autoFocus
                disabled={loading}
              />
            </div>

            <DialogFooter className="pt-4">
              <Button 
                type="button" 
                variant="outline" 
                onClick={() => onOpenChange(false)}
                disabled={loading}
                className="text-xs font-medium px-6"
              >
                Cancel
              </Button>
              <Button type="submit" disabled={!licenseKey.trim()} className="text-xs font-medium px-6">
                Next Step
              </Button>
            </DialogFooter>
          </form>
        ) : (
          <form onSubmit={handleActivate} className="space-y-4 py-4">
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-1.5">
                <Label htmlFor="m-admin-first" className="text-xs font-medium text-muted-foreground">First Name</Label>
                <Input id="m-admin-first" required value={adminForm.firstName} 
                  onChange={(e) => setAdminForm(f => ({ ...f, firstName: e.target.value }))} className="h-10 text-sm" />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="m-admin-last" className="text-xs font-medium text-muted-foreground">Last Name</Label>
                <Input id="m-admin-last" required value={adminForm.lastName} 
                  onChange={(e) => setAdminForm(f => ({ ...f, lastName: e.target.value }))} className="h-10 text-sm" />
              </div>
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="m-admin-email" className="text-xs font-medium text-muted-foreground">Email Address</Label>
              <Input id="m-admin-email" type="email" required value={adminForm.email} 
                onChange={(e) => setAdminForm(f => ({ ...f, email: e.target.value }))} className="h-10 text-sm" />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="m-admin-pass" className="text-xs font-medium text-muted-foreground">Password</Label>
              <Input id="m-admin-pass" type="password" required value={adminForm.password} 
                onChange={(e) => setAdminForm(f => ({ ...f, password: e.target.value }))} className="h-10 text-sm" />
            </div>

            {error && (
              <div className="rounded border border-destructive/20 bg-destructive/10 p-3 text-xs text-destructive">
                {error}
              </div>
            )}

            <DialogFooter className="pt-4 gap-2">
              <Button type="button" variant="outline" onClick={() => setStep("key")} disabled={loading} className="text-xs font-medium">
                Back
              </Button>
              <Button type="submit" disabled={loading} className="text-xs font-medium">
                {loading ? "Provisioning..." : "Complete Activation"}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}
