import { useMemo, useState } from "react";
import { DownloadCloud } from "lucide-react";
import { PageSection } from "@/components/layout/page-section";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import { PoseViewer } from "@/features/pose3d/pose-viewer";
import type { PoseLandmark } from "@/types";

function normalizeWorldLandmarks(payload: unknown): PoseLandmark[] | null {
  if (!payload || typeof payload !== "object") return null;
  const objectPayload = payload as Record<string, unknown>;

  const candidates: unknown[] = [];
  if (Array.isArray(objectPayload.WorldLandmarks)) candidates.push(objectPayload.WorldLandmarks);
  if (Array.isArray(objectPayload.worldLandmarks)) candidates.push(objectPayload.worldLandmarks);
  if (Array.isArray(objectPayload.landmarks)) candidates.push(objectPayload.landmarks);

  for (const candidate of candidates) {
    if (!Array.isArray(candidate)) continue;

    const first = candidate[0];
    const points = Array.isArray(first) ? first : candidate;
    if (!Array.isArray(points)) continue;

    const normalized = points
      .map((point) => {
        if (!point || typeof point !== "object") return null;
        const parsed = point as Record<string, unknown>;
        const x = Number(parsed.x);
        const y = Number(parsed.y);
        const z = Number(parsed.z);
        if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) return null;
        return {
          x,
          y,
          z,
          visibility: Number.isFinite(Number(parsed.visibility)) ? Number(parsed.visibility) : undefined,
          presence: Number.isFinite(Number(parsed.presence)) ? Number(parsed.presence) : undefined,
        } as PoseLandmark;
      })
      .filter((point): point is PoseLandmark => point !== null);

    if (normalized.length >= 33) {
      return normalized.slice(0, 33);
    }
  }

  return null;
}

export function Pose3DPage() {
  const [endpointUrl, setEndpointUrl] = useState("");
  const [jsonInput, setJsonInput] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loadedAt, setLoadedAt] = useState<string | null>(null);
  const [landmarks, setLandmarks] = useState<PoseLandmark[] | null>(null);

  const confidenceStats = useMemo(() => {
    if (!landmarks) return null;
    const confidences = landmarks.map((landmark) => Math.min(landmark.visibility ?? 1, landmark.presence ?? 1));
    const average = confidences.reduce((acc, current) => acc + current, 0) / confidences.length;
    return {
      points: landmarks.length,
      avgConfidence: average,
    };
  }, [landmarks]);

  function parseAndLoad(raw: string) {
    setError(null);
    try {
      const parsed = JSON.parse(raw);
      const normalized = normalizeWorldLandmarks(parsed);
      if (!normalized) {
        throw new Error("No valid world landmark payload found. Expected WorldLandmarks/worldLandmarks with x,y,z.");
      }
      setLandmarks(normalized);
      setLoadedAt(new Date().toISOString());
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  async function loadFromEndpoint() {
    setError(null);
    try {
      const response = await fetch(endpointUrl);
      if (!response.ok) {
        throw new Error(`Endpoint responded with HTTP ${response.status}`);
      }
      const payload = await response.text();
      setJsonInput(payload);
      parseAndLoad(payload);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <div className="space-y-6">
      <PageSection title="MediaPipe-Compatible 3D Pose Input" description="Provide real landmark payloads and render deterministic 3D skeleton geometry with confidence gating.">
        <div className="grid gap-4 md:grid-cols-[1fr_auto]">
          <div className="space-y-2">
            <Label htmlFor="pose-endpoint">JSON Endpoint (optional)</Label>
            <Input id="pose-endpoint" value={endpointUrl} onChange={(e) => setEndpointUrl(e.target.value)} placeholder="http://localhost:3000/api/v1/pose/latest" />
          </div>
          <div className="flex items-end">
            <Button disabled={!endpointUrl} onClick={loadFromEndpoint}>
              <DownloadCloud className="mr-2 h-4 w-4" />
              Fetch Payload
            </Button>
          </div>
        </div>

        <div className="mt-4 space-y-2">
          <Label htmlFor="pose-json">Landmark JSON</Label>
          <Textarea
            id="pose-json"
            value={jsonInput}
            onChange={(e) => setJsonInput(e.target.value)}
            placeholder='Paste JSON with "WorldLandmarks" or "worldLandmarks" (33 points with x,y,z)'
            className="min-h-[180px] font-mono text-xs"
          />
        </div>

        <div className="mt-4 flex flex-wrap gap-2">
          <Button onClick={() => parseAndLoad(jsonInput)} disabled={!jsonInput.trim()}>
            Render 3D Pose
          </Button>
        </div>

        <div className="mt-4 flex flex-wrap gap-2">
          {confidenceStats ? (
            <>
              <Badge variant="outline">Points: {confidenceStats.points}</Badge>
              <Badge variant={confidenceStats.avgConfidence >= 0.6 ? "success" : "warning"}>
                Avg Confidence: {(confidenceStats.avgConfidence * 100).toFixed(1)}%
              </Badge>
              <Badge variant="secondary">Loaded: {loadedAt ?? "N/A"}</Badge>
            </>
          ) : (
            <Badge variant="warning">No pose loaded</Badge>
          )}
        </div>
      </PageSection>

      <PageSection title="3D Pose Viewer" description="Canonical landmark connectivity, no randomized topology, no synthetic fallback.">
        <PoseViewer landmarks={landmarks} />
      </PageSection>

      {error ? <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">{error}</p> : null}
    </div>
  );
}

