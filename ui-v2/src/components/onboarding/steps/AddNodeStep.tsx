/**
 * Step B — Add a node (W3). Branches into ESP32 or Raspberry Pi setup.
 *
 * On entry it scans USB serial ports so a connected ESP32 is surfaced up front
 * ("Detected ESP32 on COM5 — set it up") instead of making the user hunt. A Pi
 * is a network device (added over SSH), not USB serial, so it is offered
 * separately.
 */

import { useCallback, useEffect, useState } from "react";
import { Cpu, Loader2, RefreshCw, Server, Usb } from "lucide-react";
import { Button } from "@/components/ui/button";
import { StepShell } from "../step-shell";
import type { StepProps } from "../step-shell";
import { tauriApi } from "@/lib/tauri-api";
import type { SerialPortInfo } from "@/types";
import { Esp32Setup } from "./Esp32Setup";
import { PiSetup } from "./PiSetup";

type Branch = "choose" | "esp32" | "pi";

function portLabel(p: SerialPortInfo): string {
  const maker = p.manufacturer?.trim();
  return maker ? `${p.name} (${maker})` : p.name;
}

export function AddNodeStep({ accessToken, serverStatus }: StepProps) {
  const [branch, setBranch] = useState<Branch>("choose");
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [scanning, setScanning] = useState(false);
  const [scanned, setScanned] = useState(false);

  const scan = useCallback(async () => {
    if (!accessToken) {
      setScanned(true);
      return;
    }
    setScanning(true);
    try {
      const found = await tauriApi.listSerialPorts(accessToken);
      setPorts(found);
    } catch {
      setPorts([]);
    } finally {
      setScanning(false);
      setScanned(true);
    }
  }, [accessToken]);

  // Scan whenever the chooser is shown (initial entry and on "back").
  useEffect(() => {
    if (branch === "choose") void scan();
  }, [branch, scan]);

  const esp32Ports = ports.filter((p) => p.is_esp32_compatible);
  const otherPorts = ports.filter((p) => !p.is_esp32_compatible);

  return (
    <StepShell
      title="Add a sensing node"
      description="Bring your first node online. Pick the kind of hardware you have — you can add more later from the Pi Nodes and Flash pages."
    >
      {branch === "choose" ? (
        <div className="space-y-3">
          {/* USB auto-detection banner */}
          <div className="rounded-lg border border-border/60 bg-secondary/10 p-3">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2 text-sm font-medium">
                <Usb className="h-4 w-4 text-primary" />
                USB scan
              </div>
              <Button variant="ghost" size="sm" onClick={() => void scan()} disabled={scanning || !accessToken}>
                {scanning ? <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="mr-1 h-3.5 w-3.5" />}
                Rescan
              </Button>
            </div>
            <div className="mt-1.5 text-xs">
              {scanning ? (
                <span className="text-muted-foreground">Scanning USB ports…</span>
              ) : esp32Ports.length > 0 ? (
                <div className="space-y-2">
                  <p className="text-emerald-400">
                    Detected {esp32Ports.length} ESP32-class device{esp32Ports.length > 1 ? "s" : ""} on USB:{" "}
                    <span className="text-foreground">{esp32Ports.map(portLabel).join(", ")}</span>
                  </p>
                  <Button size="sm" onClick={() => setBranch("esp32")}>
                    Set up ESP32 →
                  </Button>
                </div>
              ) : otherPorts.length > 0 ? (
                <p className="text-amber-500">
                  Found a USB serial device ({otherPorts.map((p) => p.name).join(", ")}) but it doesn't look like an ESP32.
                  Use a data cable and the board's UART port, then Rescan — or pick a node type below.
                </p>
              ) : scanned ? (
                <p className="text-muted-foreground">
                  {accessToken
                    ? "No ESP32 detected on USB. Plug one in with a data cable, or add a Raspberry Pi over the network below."
                    : "Sign in to scan for USB-connected boards."}
                </p>
              ) : (
                <span className="text-muted-foreground">…</span>
              )}
            </div>
          </div>

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
              <p className="text-xs text-muted-foreground">Reachable over the network (SSH), not USB. We'll install Nexmon CSI and start the agent.</p>
            </button>
          </div>
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
