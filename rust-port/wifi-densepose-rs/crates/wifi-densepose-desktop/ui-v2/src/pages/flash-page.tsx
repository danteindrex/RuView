import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, Loader2 } from "lucide-react";
import { tauriApi } from "@/lib/tauri-api";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import { JsonViewer } from "@/components/layout/json-viewer";
import type { ChipInfo, EspflashInfo, FlashProgress, FlashResult, VerifyResult } from "@/types";

export function FlashPage() {
  const [loading, setLoading] = useState(false);
  const [port, setPort] = useState("");
  const [firmwarePath, setFirmwarePath] = useState("");
  const [chip, setChip] = useState("esp32");
  const [baud, setBaud] = useState("921600");
  const [progress, setProgress] = useState<FlashProgress | null>(null);
  const [result, setResult] = useState<FlashResult | null>(null);
  const [verifyResult, setVerifyResult] = useState<VerifyResult | null>(null);
  const [espflash, setEspflash] = useState<EspflashInfo | null>(null);
  const [chips, setChips] = useState<ChipInfo[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const [tool, supported] = await Promise.all([tauriApi.checkEspflash(), tauriApi.supportedChips()]);
        setEspflash(tool);
        setChips(supported);
        if (supported.length > 0) {
          setChip(supported[0].id);
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      }
    })();
  }, []);

  async function pickFirmware() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Firmware", extensions: ["bin"] }],
    });
    if (typeof selected === "string") {
      setFirmwarePath(selected);
    }
  }

  async function handleFlash() {
    setLoading(true);
    setError(null);
    setResult(null);
    setProgress(null);
    try {
      const poll = setInterval(async () => {
        try {
          const current = await tauriApi.flashProgress();
          setProgress(current);
        } catch {
          // ignore background poll failures while command runs
        }
      }, 800);

      const flashResult = await tauriApi.flashFirmware({
        port,
        firmwarePath,
        chip,
        baud: Number(baud || "921600"),
      });
      clearInterval(poll);
      setResult(flashResult);
      const finalProgress = await tauriApi.flashProgress().catch(() => null);
      setProgress(finalProgress);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleVerify() {
    setLoading(true);
    setError(null);
    setVerifyResult(null);
    try {
      const check = await tauriApi.verifyFirmware({ port, firmwarePath, chip });
      setVerifyResult(check);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="space-y-6">
      <PageSection title="Flash Toolchain Health" description="Validate local espflash binary and supported chip targets before writing firmware.">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant={espflash?.installed ? "success" : "danger"}>{espflash?.installed ? "espflash available" : "espflash unavailable"}</Badge>
          {espflash?.version ? <Badge variant="outline">{espflash.version}</Badge> : null}
          {espflash?.path ? <Badge variant="secondary">{espflash.path}</Badge> : null}
        </div>
      </PageSection>

      <PageSection title="Firmware Flash" description="Write firmware to target board with explicit chip and baud control.">
        <div className="grid gap-4 md:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="flash-port">Serial Port</Label>
            <Input id="flash-port" value={port} onChange={(e) => setPort(e.target.value)} placeholder="COM3 or /dev/ttyUSB0" />
          </div>
          <div className="space-y-2">
            <Label htmlFor="flash-baud">Baud</Label>
            <Input id="flash-baud" value={baud} onChange={(e) => setBaud(e.target.value)} />
          </div>
          <div className="space-y-2">
            <Label htmlFor="flash-chip">Chip</Label>
            <Select value={chip} onValueChange={setChip}>
              <SelectTrigger id="flash-chip">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {chips.map((supportedChip) => (
                  <SelectItem key={supportedChip.id} value={supportedChip.id}>
                    {supportedChip.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label htmlFor="flash-path">Firmware (.bin)</Label>
            <div className="flex gap-2">
              <Input id="flash-path" value={firmwarePath} onChange={(e) => setFirmwarePath(e.target.value)} />
              <Button variant="outline" onClick={pickFirmware}>
                Browse
              </Button>
            </div>
          </div>
        </div>

        <div className="mt-4 flex flex-wrap gap-2">
          <Button disabled={loading || !port || !firmwarePath} onClick={handleFlash}>
            {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : null}
            Flash Firmware
          </Button>
          <Button disabled={loading || !port || !firmwarePath} variant="secondary" onClick={handleVerify}>
            Verify Firmware
          </Button>
        </div>
      </PageSection>

      {progress ? (
        <PageSection title="Flash Progress" description="Current operation state reported by flash service.">
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span>{progress.phase}</span>
              <span>{progress.progress_pct.toFixed(1)}%</span>
            </div>
            <Progress value={progress.progress_pct} />
            {progress.message ? <p className="text-sm text-muted-foreground">{progress.message}</p> : null}
          </div>
        </PageSection>
      ) : null}

      {result ? (
        <PageSection title="Flash Result" description="Final response from firmware flashing command.">
          <div className="mb-3 flex items-center gap-2 text-sm text-emerald-400">
            <CheckCircle2 className="h-4 w-4" />
            {result.message}
          </div>
          <JsonViewer value={result} />
        </PageSection>
      ) : null}

      {verifyResult ? (
        <PageSection title="Verification Result" description="Post-flash verification output.">
          <JsonViewer value={verifyResult} />
        </PageSection>
      ) : null}

      {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
    </div>
  );
}

