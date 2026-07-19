/**
 * ESP32 first-run setup (W3, step B — ESP32 branch).
 *
 * Flow: pick serial port (auto-select an ESP32-compatible one) → optionally
 * flash the 4-segment firmware bundle with live progress → fill a WiFi form
 * (node_id auto-incremented, target_ip best-effort prefilled) → provision NVS →
 * watch /api/v1/nodes until the new node streams.
 *
 * Flashing is optional: "already flashed → just provision" and "already set up
 * → skip". Every backend call is guarded so a missing sibling command or a
 * down server degrades to a disabled/"unavailable" state, never a crash.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { CheckCircle2, Loader2, RadioTower, RefreshCw, Usb, Wifi, Zap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { tauriApi } from "@/lib/tauri-api";
import { isLoopbackHostPort } from "@/lib/utils";
import { StatusLine } from "../step-shell";
import { deriveHubLanIp, fetchServerNodes, nodeIdNumber, resolveServerBase } from "../server-api";
import { useNodeWatch } from "../use-node-watch";
import type { FlashProgress, SerialPortInfo, ServerStatusResponse, WifiNetwork } from "@/types";

type Phase = "form" | "flashing" | "provisioning" | "watching" | "done";

interface Esp32SetupProps {
  accessToken: string | null;
  serverStatus: ServerStatusResponse | null;
}

export function Esp32Setup({ accessToken, serverStatus }: Esp32SetupProps) {
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [port, setPort] = useState("");
  const [chip, setChip] = useState("esp32s3");
  const [flashMode, setFlashMode] = useState<"flash" | "provision-only">("flash");

  const [ssid, setSsid] = useState("");
  const [password, setPassword] = useState("");
  const [targetIp, setTargetIp] = useState("");
  const [nodeId, setNodeId] = useState(1);
  const [wifiList, setWifiList] = useState<WifiNetwork[]>([]);
  const [scanningWifi, setScanningWifi] = useState(false);

  const [phase, setPhase] = useState<Phase>("form");
  const [progress, setProgress] = useState<FlashProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // Result of the post-provision serial check: did the node actually join WiFi?
  const [joinInfo, setJoinInfo] = useState<{ joined: boolean; ip: string | null } | null>(null);

  const base = useMemo(() => resolveServerBase(serverStatus), [serverStatus]);
  const watch = useNodeWatch({ base, targetId: nodeId, enabled: phase === "watching", timeoutMs: 120000 });

  // Load serial ports + auto-select an ESP32-compatible one.
  useEffect(() => {
    if (!accessToken) return;
    let cancelled = false;
    void (async () => {
      try {
        const found = await tauriApi.listSerialPorts(accessToken);
        if (cancelled) return;
        setPorts(found);
        const auto = found.find((p) => p.is_esp32_compatible) ?? found[0];
        if (auto) setPort(auto.name);
      } catch {
        // command unavailable / no ports — leave the manual field usable
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [accessToken]);

  // Prefill WiFi SSID + hub LAN IP from the host PC's current network, and
  // auto-fill the saved WiFi password for that network from the Windows profile.
  // Without the password prefill the user can click Provision with a blank
  // password — the firmware then tries to join WPA2 with no key, the join fails,
  // and the node silently never streams (the #1 "watching forever" failure).
  // This mirrors what the setup-esp32-node.ps1 script does to make node 1 work.
  useEffect(() => {
    if (!accessToken) return;
    let cancelled = false;
    void (async () => {
      try {
        const info = await tauriApi.hostNetworkInfo(accessToken);
        if (cancelled) return;
        if (info.ssid) setSsid((prev) => prev || info.ssid || "");
        if (info.lan_ip) setTargetIp((prev) => prev || info.lan_ip || "");
        if (info.ssid) {
          try {
            const pw = await tauriApi.wifiSavedPassword(accessToken, info.ssid);
            if (!cancelled && pw) setPassword((prev) => prev || pw);
          } catch {
            // no saved key for this network — the user types it
          }
        }
      } catch {
        // command unavailable — leave the fields manual
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [accessToken]);

  // Scan nearby WiFi so the user can pick a network (auto-fills SSID + the saved
  // password) instead of typing — the reliable way to avoid a wrong password.
  async function rescanWifi() {
    if (!accessToken) return;
    setScanningWifi(true);
    try {
      setWifiList(await tauriApi.scanWifiNetworks(accessToken));
    } catch {
      // command unavailable — the manual SSID field stays usable
    } finally {
      setScanningWifi(false);
    }
  }
  useEffect(() => {
    if (!accessToken) return;
    let cancelled = false;
    void (async () => {
      setScanningWifi(true);
      try {
        const nets = await tauriApi.scanWifiNetworks(accessToken);
        if (!cancelled) setWifiList(nets);
      } catch {
        // unavailable
      } finally {
        if (!cancelled) setScanningWifi(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [accessToken]);

  async function selectNetwork(net: WifiNetwork) {
    setSsid(net.ssid);
    if (net.saved && accessToken) {
      try {
        const pw = await tauriApi.wifiSavedPassword(accessToken, net.ssid);
        if (pw) setPassword(pw);
      } catch {
        // couldn't read saved password — user types it
      }
    }
  }

  // Auto-increment node_id from the current roster + prefill hub IP.
  useEffect(() => {
    setTargetIp((prev) => prev || deriveHubLanIp(serverStatus));
    if (!base) return;
    let cancelled = false;
    void (async () => {
      try {
        const nodes = await fetchServerNodes(base);
        if (cancelled) return;
        const ids = nodes.map(nodeIdNumber).filter((n): n is number => n !== null);
        const next = ids.length ? Math.max(...ids) + 1 : 1;
        setNodeId((prev) => (prev === 1 ? next : prev));
      } catch {
        // no roster yet — keep default id 1
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [base, serverStatus]);

  // Advance to "done" once the watched node appears.
  useEffect(() => {
    if (phase === "watching" && watch.state === "found") setPhase("done");
  }, [phase, watch.state]);

  const flashPollRef = useRef<number | null>(null);
  function stopFlashPoll() {
    if (flashPollRef.current !== null) {
      window.clearInterval(flashPollRef.current);
      flashPollRef.current = null;
    }
  }
  useEffect(() => () => stopFlashPoll(), []);

  const targetPort = serverStatus?.udp_port ?? 5005;

  async function handleRun() {
    setError(null);
    setNotice(null);
    if (!accessToken) {
      setError("Not authenticated — sign in to run hardware setup.");
      return;
    }
    if (!port) {
      setError("Select or enter the serial port your ESP32 is on.");
      return;
    }
    if (!ssid) {
      setError("Enter or pick the 2.4GHz WiFi network the node should join.");
      return;
    }
    // A blank password on a secured network provisions a node that can never
    // join WiFi and then silently never streams. Allow blank ONLY when the
    // chosen SSID is a known OPEN network from the scan.
    const scanned = wifiList.find((n) => n.ssid === ssid);
    if (!password && !scanned?.open) {
      setError(
        "Enter the WiFi password — or pick your network from “Scan nearby WiFi” to auto-fill it. " +
          "A blank password can't join a secured network, so the node would never come online.",
      );
      return;
    }
    if (targetIp && isLoopbackHostPort(targetIp)) {
      setError("Hub IP is a loopback address — set it to this PC's LAN IP so the node can reach it.");
      return;
    }

    try {
      // 1. Flash (optional)
      if (flashMode === "flash") {
        setPhase("flashing");
        setProgress(null);
        stopFlashPoll();
        flashPollRef.current = window.setInterval(async () => {
          try {
            const current = await tauriApi.flashProgress(accessToken);
            setProgress(current);
          } catch {
            // ignore poll failures while the command runs
          }
        }, 800);

        const release = await tauriApi.fetchFirmwareRelease(accessToken);
        await tauriApi.flashFirmwareBundle(accessToken, {
          port,
          chip,
          segments: release.segments,
          baud: 921600,
        });
        stopFlashPoll();
        const finalProgress = await tauriApi.flashProgress(accessToken).catch(() => null);
        setProgress(finalProgress);
      }

      // 2. Provision NVS over serial
      setPhase("provisioning");
      await tauriApi.provisionEsp32Nvs(accessToken, {
        port,
        chip,
        config: {
          ssid,
          password,
          target_ip: targetIp,
          target_port: targetPort,
          node_id: nodeId,
        },
        baud: 115200,
      });

      // 2b. Verify the node actually joined WiFi by reading its serial console
      //     (mirrors setup-esp32-node.ps1). This turns a later "watching
      //     forever" into a concrete cause: not joined ⇒ wrong password / 5GHz
      //     network; joined ⇒ the network path (firewall / wrong hub IP) is the
      //     problem, not the node. Best-effort — a missing command/port does not
      //     block the flow, it just falls through to the server watch.
      setJoinInfo(null);
      const check = await tauriApi.esp32SerialCheck(accessToken, port, 30).catch(() => null);
      if (check) {
        setJoinInfo({ joined: check.joined, ip: check.ip });
        if (!check.joined) {
          setError(
            `Node provisioned, but it did not join "${ssid}" within 30s. ` +
              "Almost always the WiFi password is wrong, or the network is 5GHz-only " +
              "(the ESP32 needs 2.4GHz). Fix the password/network and provision again." +
              (check.log_tail ? `\n\nLast serial output:\n${check.log_tail.slice(-400)}` : ""),
          );
          setPhase("form");
          return;
        }
        setNotice(
          `Node joined WiFi${check.ip ? ` (got IP ${check.ip})` : ""}. Waiting for its data to reach this PC…`,
        );
      }

      // 3. Watch for the node to stream in
      if (!base) {
        setNotice("Node provisioned. Start the sensing server to watch it come online.");
        setPhase("done");
        return;
      }
      setPhase("watching");
    } catch (err) {
      stopFlashPoll();
      setError(err instanceof Error ? err.message : String(err));
      setPhase("form");
    }
  }

  const busy = phase === "flashing" || phase === "provisioning";
  const selectedPortInfo = ports.find((p) => p.name === port);

  return (
    <div className="space-y-4">
      {/* Port picker */}
      <div className="rounded-lg border border-border/60 bg-secondary/20 p-3">
        <div className="mb-2 flex items-center gap-2 text-sm font-medium">
          <Usb className="h-4 w-4 text-primary" /> Serial port
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5">
            <Label htmlFor="esp-port">Port</Label>
            {ports.length > 0 ? (
              <Select value={port} onValueChange={setPort}>
                <SelectTrigger id="esp-port">
                  <SelectValue placeholder="Select a port" />
                </SelectTrigger>
                <SelectContent>
                  {ports.map((p) => (
                    <SelectItem key={p.name} value={p.name}>
                      {p.name}
                      {p.is_esp32_compatible ? " · ESP32" : ""}
                      {p.manufacturer ? ` · ${p.manufacturer}` : ""}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <Input id="esp-port" value={port} onChange={(e) => setPort(e.target.value)} placeholder="COM7 or /dev/ttyUSB0" />
            )}
            {selectedPortInfo && !selectedPortInfo.is_esp32_compatible ? (
              <p className="text-xs text-amber-500">This port doesn't look like an ESP32 — double-check it's the right one.</p>
            ) : null}
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="esp-chip">Chip</Label>
            <Select value={chip} onValueChange={setChip}>
              <SelectTrigger id="esp-chip">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="esp32s3">ESP32-S3</SelectItem>
                <SelectItem value="esp32c6">ESP32-C6</SelectItem>
                <SelectItem value="esp32">ESP32</SelectItem>
                <SelectItem value="esp32s2">ESP32-S2</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </div>
      </div>

      {/* Flash mode */}
      <div className="flex flex-wrap gap-2">
        <Button variant={flashMode === "flash" ? "default" : "outline"} size="sm" onClick={() => setFlashMode("flash")} disabled={busy}>
          <Zap className="mr-2 h-4 w-4" /> Flash firmware then provision
        </Button>
        <Button
          variant={flashMode === "provision-only" ? "default" : "outline"}
          size="sm"
          onClick={() => setFlashMode("provision-only")}
          disabled={busy}
        >
          My node is already flashed — just provision
        </Button>
      </div>

      {/* WiFi form */}
      <div className="rounded-lg border border-border/60 bg-secondary/20 p-3">
        <div className="mb-2 flex items-center gap-2 text-sm font-medium">
          <RadioTower className="h-4 w-4 text-primary" /> Network + identity
        </div>
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-1.5 sm:col-span-2">
            <div className="flex items-center justify-between">
              <Label className="flex items-center gap-1.5">
                <Wifi className="h-3.5 w-3.5 text-primary" /> Scan nearby WiFi
              </Label>
              <Button variant="ghost" size="sm" onClick={() => void rescanWifi()} disabled={scanningWifi || !accessToken}>
                {scanningWifi ? <Loader2 className="mr-1 h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="mr-1 h-3.5 w-3.5" />}
                Rescan
              </Button>
            </div>
            {wifiList.length > 0 ? (
              <Select
                onValueChange={(v) => {
                  const n = wifiList.find((w) => w.ssid === v);
                  if (n) void selectNetwork(n);
                }}
              >
                <SelectTrigger>
                  <SelectValue placeholder="Pick a network — 2.4GHz only, saved ones auto-fill the password" />
                </SelectTrigger>
                <SelectContent>
                  {wifiList.map((n) => (
                    <SelectItem key={n.ssid} value={n.ssid} disabled={!n.band_24ghz}>
                      {n.ssid}
                      {n.connected ? " · connected" : ""}
                      {n.band_24ghz ? " · 2.4GHz" : " · 5GHz only (ESP32 can't join)"}
                      {n.saved ? " · saved" : ""}
                      {n.open ? " · open" : ""}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            ) : (
              <p className="text-xs text-muted-foreground">
                {scanningWifi ? "Scanning for networks…" : "No networks found — type the SSID below."}
              </p>
            )}
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="esp-ssid">WiFi SSID</Label>
            <Input id="esp-ssid" value={ssid} onChange={(e) => setSsid(e.target.value)} placeholder="Your 2.4GHz network" />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="esp-pass">WiFi Password</Label>
            <Input id="esp-pass" type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="esp-ip">Hub IP (this PC)</Label>
            <Input id="esp-ip" value={targetIp} onChange={(e) => setTargetIp(e.target.value)} placeholder="192.168.1.20" />
            {targetIp && isLoopbackHostPort(targetIp) ? (
              <p className="text-xs text-amber-500">Loopback won't work — the node needs this PC's LAN IP.</p>
            ) : null}
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="esp-node">Node ID</Label>
            <Input
              id="esp-node"
              inputMode="numeric"
              value={String(nodeId)}
              onChange={(e) => {
                const raw = e.target.value.trim();
                const n = Number(raw);
                // Reject empty ("" → 0) and non-integers so we never provision or
                // watch for node id 0 / a fractional id.
                if (raw !== "" && Number.isInteger(n) && n >= 0) setNodeId(n);
              }}
            />
          </div>
        </div>
      </div>

      {progress ? (
        <div className="space-y-1.5">
          <div className="flex items-center justify-between text-xs">
            <span>{progress.phase}</span>
            <span>{progress.progress_pct.toFixed(0)}%</span>
          </div>
          <Progress value={progress.progress_pct} />
          {progress.message ? <p className="text-xs text-muted-foreground">{progress.message}</p> : null}
        </div>
      ) : null}

      {phase === "watching" && watch.state === "watching" ? (
        <StatusLine tone="muted">
          <Loader2 className="mr-2 inline h-4 w-4 animate-spin" /> Watching for node {nodeId} to stream… ({watch.elapsedSecs}s)
        </StatusLine>
      ) : null}

      {phase === "watching" && watch.state === "timeout" ? (
        <StatusLine tone="warn">
          {joinInfo?.joined ? (
            <>
              Node {nodeId} joined WiFi{joinInfo.ip ? ` (IP ${joinInfo.ip})` : ""} but its data isn't reaching this PC. This is
              almost always <strong>Windows Firewall</strong> blocking UDP {targetPort} for this app (allow it on Private
              networks), or the <strong>Hub IP</strong> ({targetIp || "unset"}) isn't this PC's current LAN address. Fix that and
              provision again — the node itself is fine.
            </>
          ) : (
            <>
              Node {nodeId} hasn't appeared yet. Check the node's power and WiFi, then confirm the SSID/password. It will show up
              on the dashboard once it connects.
            </>
          )}
        </StatusLine>
      ) : null}

      {phase === "done" ? (
        <StatusLine tone="ok">
          <CheckCircle2 className="mr-2 inline h-4 w-4" /> {notice ?? `Node ${nodeId} is set up. Continue to place it in 3D.`}
        </StatusLine>
      ) : null}

      {notice && phase !== "done" ? <StatusLine tone="muted">{notice}</StatusLine> : null}
      {error ? <StatusLine tone="error">{error}</StatusLine> : null}

      <div className="flex flex-wrap gap-2">
        <Button onClick={handleRun} disabled={busy || !accessToken}>
          {busy ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <Zap className="mr-2 h-4 w-4" />}
          {flashMode === "flash" ? "Flash & provision node" : "Provision node"}
        </Button>
      </div>
    </div>
  );
}
