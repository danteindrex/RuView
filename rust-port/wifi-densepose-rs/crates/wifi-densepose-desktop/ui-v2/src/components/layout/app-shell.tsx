import React from "react";
import { MoonStar, PanelLeftClose, PanelLeftOpen, SunMedium } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";

export interface ShellPage {
  id: string;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
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
              <p className="text-xs uppercase tracking-[0.2em] text-primary">Wave</p>
              <h1 className="text-xl font-semibold tracking-tight">Control Plane v2</h1>
              <p className="text-sm text-muted-foreground">{subtitle}</p>
            </div>
          ) : null}
          <Button
            variant="outline"
            size="icon"
            onClick={() => setSidebarCollapsed((current) => !current)}
            aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          >
            {sidebarCollapsed ? <PanelLeftOpen className="h-4 w-4" /> : <PanelLeftClose className="h-4 w-4" />}
          </Button>
        </div>

        <nav className="space-y-1">
          {pages.map((page) => {
            const Icon = page.icon;
            const active = page.id === activePage;
            return (
              <button
                key={page.id}
                type="button"
                className={cn(
                  "flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left text-sm transition-colors",
                  sidebarCollapsed && "justify-center px-2",
                  active
                    ? "bg-primary/20 text-primary ring-1 ring-primary/40"
                    : "text-muted-foreground hover:bg-secondary/70 hover:text-foreground",
                )}
                onClick={() => onPageChange(page.id)}
                title={sidebarCollapsed ? page.label : undefined}
              >
                <Icon className="h-4 w-4" />
                {!sidebarCollapsed ? <span>{page.label}</span> : null}
              </button>
            );
          })}
        </nav>

        <div className={cn("mt-auto space-y-4", sidebarCollapsed && "hidden")}>
          <Separator />
          <div className="space-y-2 text-xs">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Sensing Server</span>
              <Badge variant={serverRunning ? "success" : "danger"}>{serverRunning ? "Running" : "Stopped"}</Badge>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Online Nodes</span>
              <Badge variant={onlineNodes > 0 ? "success" : "warning"}>
                {onlineNodes}/{totalNodes}
              </Badge>
            </div>
          </div>
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        <header className={cn("sticky top-0 z-20 border-b border-border/60 bg-background/70 px-4 py-3 backdrop-blur md:px-6", immersive && "md:hidden")}>
          <div className="mb-3 md:hidden">
            <label className="mb-1 block text-xs text-muted-foreground" htmlFor="mobile-page-select">
              Navigation
            </label>
            <select
              id="mobile-page-select"
              className="h-10 w-full rounded-md border border-input bg-background/70 px-3 text-sm"
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
              <h2 className="text-lg font-semibold tracking-tight">{title}</h2>
              <p className="text-sm text-muted-foreground">{subtitle}</p>
            </div>
            <Button variant="outline" size="icon" onClick={onThemeToggle} aria-label="Toggle theme">
              {theme === "dark" ? <SunMedium className="h-4 w-4" /> : <MoonStar className="h-4 w-4" />}
            </Button>
          </div>
        </header>
        <main className={cn("min-h-0 flex-1 overflow-auto", immersive ? "p-0" : "p-4 md:p-6")}>{children}</main>
      </section>
    </div>
  );
}
