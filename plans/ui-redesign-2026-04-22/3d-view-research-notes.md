# 3D View Research Notes (Chrome MCP)

Date: 2026-04-22  
Method: browser research using Chrome DevTools MCP

## Sources Reviewed

Primary:
- https://ai.google.dev/edge/mediapipe/solutions/vision/pose_landmarker
- https://ai.google.dev/edge/mediapipe/solutions/vision/pose_landmarker/web_js
- https://github.com/google-ai-edge/mediapipe-samples-web/blob/main/src/tasks/pose-landmarker.ts

Secondary:
- https://github.com/bandinopla/three-mediapipe-rig
- https://threejs.org/docs/#api/en/helpers/SkeletonHelper

## What Matters for Our Redesign

1. Use world landmarks, not only normalized 2D points.
- MediaPipe Pose Landmarker exposes both:
  - `Landmarks` (normalized image coordinates)
  - `WorldLandmarks` (3D coordinates in meters, hip-midpoint origin)
- This gives us physically meaningful 3D pose structure instead of arbitrary graph points.

2. Use canonical connector topology.
- MediaPipe sample rendering uses `PoseLandmarker.POSE_CONNECTIONS` and explicit landmark connector drawing.
- This avoids ambiguous/custom edge definitions and keeps pose display semantically correct.

3. Confidence-aware rendering.
- Results include `visibility` / `presence` confidence.
- Render rule: de-emphasize low-confidence joints and edges rather than showing all equally.

4. Move heavy inference/render logic off the main UI thread where possible.
- Web guide notes synchronous calls can block the main thread during video processing.
- For desktop webview UI, detection/render updates should be throttled and isolated to avoid UI stalls.

5. Rig retargeting is feasible, but should be phase 2.
- `three-mediapipe-rig` demonstrates tracker-to-skeleton mapping with custom bone maps.
- Good reference for future avatar/rig support, but production MVP should first ship a robust landmark skeleton viewer.

## Recommended 3D Architecture (for new UI)

Phase A (ship first):
- Build deterministic 3D pose canvas:
  - Landmark points from `WorldLandmarks`
  - Connector lines from canonical pose connections
  - Confidence thresholding and fading
  - Camera orbit + reset + top/front/side views
  - Scale/axis helpers and fixed units

Phase B (after stability):
- Add optional rig retarget preview:
  - SkeletonHelper overlays
  - Bone map configuration
  - Per-joint smoothing and clamping

## Explicit Rejection

- No random-force topology for pose.
- No fabricated motion metrics without verifiable backend source.
- No “demo look” styling in operations view.

