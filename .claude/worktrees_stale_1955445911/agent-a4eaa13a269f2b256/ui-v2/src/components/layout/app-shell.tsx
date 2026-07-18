import React from "react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import type { AuthUser } from "@/lib/auth-api";

export interface ShellPage {
  id: string;
  label: string;
  icon?: React.ElementType;
}

interface AppShellProps {
  pages: ShellPage[];
  activePage: string;
  onPageChange: (id: string) => void;
  title: string;
  subtitle: string;
  children: React.ReactNode;
  serverRunning: boolean;
  onlineNodes: number;
  totalNodes: number;
  theme: "dark" | "light";
  onThemeToggle: () => void;
  immersive?: boolean;
  user?: AuthUser | null;
  isSuperAdmin?: boolean;
  onLogout?: () => void;
  isLicensed?: boolean;
  onActivateLicense?: () => void;
}

export function AppShell({
  pages,
  activePage,
  onPageChange,
  title,
  subtitle,
  children,
  serverRunning,
  onlineNodes,
  totalNodes,
  theme,
  onThemeToggle,
  immersive = false,
  user,
  isSuperAdmin,
  onLogout,
  isLicensed = true,
  onActivateLicense,
}: AppShellProps) {
  const [sidebarCollapsed, setSidebarCollapsed] = React.useState(() => localStorage.getItem("wave-v2-sidebar") === "collapsed");

  React.useEffect(() => {
    localStorage.setItem("wave-v2-sidebar", sidebarCollapsed ? "collapsed" : "expanded");
  }, [sidebarCollapsed]);

  return (
    <div className="flex h-full w-full overflow-hidden">
      <aside
        className={cn(
          "hidden shrink-0 border-r border-border/60 bg-card/40 p-4 backdrop-blur transition-[width] duration-300 md:flex md:flex-col",
          sidebarCollapsed ? "w-[76px]" : "w-72",
        )}
      >
        <div className={cn("mb-6 flex items-start gap-2", sidebarCollapsed ? "justify-center" : "justify-between")}>
          {!sidebarCollapsed ? (
            <div className="space-y-1">
              <p className="text-[10px] font-bold uppercase tracking-widest text-primary">Wave</p>
              <h1 className="text-xl font-bold tracking-tight">Control Plane</h1>
            </div>
          ) : null}
          <button
            onClick={() => setSidebarCollapsed((current) => !current)}
            className="flex h-8 w-8 items-center justify-center rounded border border-border/60 text-[10px] font-bold hover:bg-secondary/70 transition-colors"
            title={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          >
            {sidebarCollapsed ? ">>" : "<<"}
          </button>
        </div>

        <nav className="space-y-1">
          {pages.map((page) => {
            const active = page.id === activePage;
            return (
              <button
                key={page.id}
                type="button"
                className={cn(
                  "flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left text-sm font-medium transition-colors",
                  sidebarCollapsed && "justify-center px-2",
                  active
                    ? "bg-primary/20 text-primary ring-1 ring-primary/40"
                    : "text-muted-foreground hover:bg-secondary/70 hover:text-foreground",
                )}
                onClick={() => onPageChange(page.id)}
                title={sidebarCollapsed ? page.label : undefined}
              >
                {!sidebarCollapsed ? (
                  <span>{page.label}</span>
                ) : (
                  page.icon ? <page.icon className="h-4 w-4" /> : <span className="text-[10px] font-bold">{page.label.substring(0, 2).toUpperCase()}</span>
                )}
              </button>
            );
          })}
        </nav>

        <div className={cn("mt-auto space-y-4", sidebarCollapsed && "hidden")}>
          <Separator />

          {/* User info & logout */}
          {user && (
            <div className="space-y-2">
              <div className="flex items-center gap-3 rounded-lg bg-secondary/30 p-3">
                <div className="flex h-8 w-8 items-center justify-center rounded border border-primary/20 bg-primary/5 text-xs font-bold text-primary">
                  {user.first_name?.[0]}{user.last_name?.[0]}
                </div>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-xs font-bold uppercase">
                    {user.first_name} {user.last_name}
                  </p>
                  <p className="truncate text-[10px] text-muted-foreground font-medium">{user.email}</p>
                </div>
              </div>
              {isSuperAdmin && (
                <div className="flex justify-center rounded border border-primary/20 bg-primary/5 py-1 text-[9px] font-bold uppercase tracking-wider text-primary">
                  Vendor Administrator
                </div>
              )}
              {onLogout && (
                <button
                  onClick={onLogout}
                  className="w-full rounded border border-border/60 py-2 text-[10px] font-bold uppercase tracking-widest text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive"
                >
                  Logout
                </button>
              )}
            </div>
          )}

          <Separator />
          <div className="space-y-2 text-[10px] font-bold uppercase tracking-wider">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Server</span>
              <span className={serverRunning ? "text-success" : "text-destructive"}>{serverRunning ? "Online" : "Offline"}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Nodes</span>
              <span className="text-foreground">
                {onlineNodes}/{totalNodes}
              </span>
            </div>
          </div>
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        {!isLicensed && (
          <div className="flex items-center justify-between bg-destructive px-6 py-2.5 text-destructive-foreground">
            <div className="flex items-center gap-4">
              <span className="text-[10px] font-black uppercase tracking-[0.2em] bg-white/20 px-2 py-0.5 rounded">Restricted Access</span>
              <p className="text-xs font-bold uppercase tracking-widest">License Status: Inactive</p>
            </div>
            <button 
              onClick={onActivateLicense}
              className="text-[10px] font-black uppercase tracking-widest bg-white text-destructive px-4 py-1 rounded hover:bg-white/90 transition-transform active:scale-95"
            >
              Activate Now
            </button>
          </div>
        )}
        <header className={cn("sticky top-0 z-20 border-b border-border/60 bg-background/70 px-4 py-3 backdrop-blur md:px-6", immersive && "md:hidden")}>
          <div className="mb-3 md:hidden">
            <select
              id="mobile-page-select"
              className="h-10 w-full rounded-md border border-input bg-background/70 px-3 text-xs font-bold uppercase tracking-wider"
              value={activePage}
              onChange={(event) => onPageChange(event.target.value)}
            >
              {pages.map((page) => (
                <option key={page.id} value={page.id}>
                  {page.label}
                </option>
              ))}
            </select>
          </div>
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-lg font-bold tracking-tight">{title}</h2>
              <p className="text-xs text-muted-foreground font-medium">{subtitle}</p>
            </div>
            <div className="flex items-center gap-3">
              <button 
                onClick={onThemeToggle} 
                className="flex h-8 px-3 items-center justify-center rounded border border-border/60 text-[10px] font-bold uppercase tracking-wider hover:bg-secondary/70 transition-colors"
              >
                {theme === "dark" ? "Mode: Light" : "Mode: Dark"}
              </button>
              {user && (
                <div className="hidden items-center gap-2 lg:flex">
                  <span className="text-[10px] font-bold uppercase text-muted-foreground">
                    Active: {user.first_name} {user.last_name}
                  </span>
                </div>
              )}
            </div>
          </div>
        </header>
        <main className={cn("min-h-0 flex-1 overflow-auto", immersive ? "p-0" : "p-4 md:p-6")}>{children}</main>
      </section>
    </div>
  );
}
