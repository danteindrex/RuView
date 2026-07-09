/**
 * Step B — Add a node (W3). Branches into ESP32 or Raspberry Pi setup.
 */

import { useState } from "react";
import { Cpu, Server } from "lucide-react";
import { Button } from "@/components/ui/button";
import { StepShell } from "../step-shell";
import type { StepProps } from "../step-shell";
import { Esp32Setup } from "./Esp32Setup";
import { PiSetup } from "./PiSetup";

type Branch = "choose" | "esp32" | "pi";

export function AddNodeStep({ accessToken, serverStatus }: StepProps) {
  const [branch, setBranch] = useState<Branch>("choose");

  return (
    <StepShell
      title="Add a sensing node"
      description="Bring your first node online. Pick the kind of hardware you have — you can add more later from the Pi Nodes and Flash pages."
    >
      {branch === "choose" ? (
        <div className="grid gap-3 sm:grid-cols-2">
          <button
            type="button"
            onClick={() => setBranch("esp32")}
            className="flex flex-col items-start gap-2 rounded-lg border border-border/60 bg-secondary/20 p-4 text-left transition-colors hover:border-primary/60"
          >
            <Cpu className="h-6 w-6 text-primary" />
            <p className="text-sm font-semibold">ESP32 node</p>
            <p className="text-xs text-muted-foreground">Plugged in over USB. We'll flash firmware (optional), set WiFi, and provision it.</p>
          </button>
          <button
            type="button"
            onClick={() => setBranch("pi")}
            className="flex flex-col items-start gap-2 rounded-lg border border-border/60 bg-secondary/20 p-4 text-left transition-colors hover:border-primary/60"
          >
            <Server className="h-6 w-6 text-primary" />
            <p className="text-sm font-semibold">Raspberry Pi node</p>
            <p className="text-xs text-muted-foreground">Reachable over SSH. We'll run the full Nexmon CSI setup and start the agent.</p>
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          <Button variant="ghost" size="sm" onClick={() => setBranch("choose")}>
            ← Choose a different node type
          </Button>
          {branch === "esp32" ? (
            <Esp32Setup accessToken={accessToken} serverStatus={serverStatus} />
          ) : (
            <PiSetup accessToken={accessToken} serverStatus={serverStatus} />
          )}
        </div>
      )}
    </StepShell>
  );
}
