/** Welcome step — sets expectations for the first-run wizard (W3). */

import { Box, Cpu, HeartPulse, Radar } from "lucide-react";
import { StepShell } from "../step-shell";
import type { StepProps } from "../step-shell";

const HIGHLIGHTS = [
  { icon: Cpu, title: "Add your sensors", body: "Flash and provision an ESP32 node, or bring a Raspberry Pi online — all inside this app." },
  { icon: Box, title: "Place them in 3D", body: "Drop each node where it physically sits so person tracking knows the room." },
  { icon: Radar, title: "Calibrate the room", body: "A short empty-room baseline teaches the system what 'nobody here' looks like." },
  { icon: HeartPulse, title: "See yourself tracked", body: "Watch live presence, pose, and vitals in the observatory." },
];

export function WelcomeStep(_props: StepProps) {
  return (
    <StepShell
      title="Welcome to Wave"
      description="This guided setup takes you from a sealed box to seeing yourself tracked live. It takes about 10 minutes; you can skip any step and re-run it later from Settings."
    >
      <div className="grid gap-3 sm:grid-cols-2">
        {HIGHLIGHTS.map((item) => (
          <div key={item.title} className="flex gap-3 rounded-lg border border-border/60 bg-secondary/20 p-3">
            <item.icon className="mt-0.5 h-5 w-5 shrink-0 text-primary" />
            <div>
              <p className="text-sm font-medium">{item.title}</p>
              <p className="text-xs text-muted-foreground">{item.body}</p>
            </div>
          </div>
        ))}
      </div>
    </StepShell>
  );
}
