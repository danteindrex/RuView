/**
 * Live-data mapper — normalizes real sensing-server WebSocket frames into the
 * demo-data person shape the Observatory renderers consume.
 *
 * The Rust PersonDetection NEVER sends position / motion_score / pose. Its
 * real fields are: id, confidence, keypoints[{name,x,y,z,confidence}],
 * bbox{x,y,width,height} (640x480 detection canvas), zone. Figure position is
 * derived from the top-level location_hint (room meters) with a bbox-center
 * fallback, and the skeleton from the top-level pose_keypoints tensor
 * [x,y,z,conf][17] in COCO-17 order (nose, left_eye, right_eye, left_ear,
 * right_ear, left_shoulder, right_shoulder, left_elbow, right_elbow,
 * left_wrist, right_wrist, left_hip, right_hip, left_knee, right_knee,
 * left_ankle, right_ankle) when present.
 */

// Scene floor extents (room is 12 x 10 units; keep figures inside the walls).
const ROOM_HALF_X = 5.5;
const ROOM_HALF_Z = 4.5;
// Rust person-detection canvas dimensions (see sensing-server main.rs).
const CANVAS_W = 640;
const CANVAS_H = 480;

// PostureClass (snake_case strings from the sensing server) → observatory pose.
const POSTURE_TO_POSE = {
  standing: 'standing',
  sitting: 'sitting',
  lying_down: 'lying',
  walking: 'walking',
};

// classification.motion_level → a demo-style motion score (0..200).
// The sensing server's raw_classify emits: absent | present_still |
// present_moving | active (see sensing-server csi.rs); extra aliases are kept
// for adaptive-classifier custom labels.
const MOTION_LEVEL_SCORE = {
  absent: 0,
  none: 0,
  present_still: 8,
  still: 8,
  idle: 8,
  breathing: 12,
  low: 20,
  moderate: 60,
  medium: 60,
  present_moving: 80,
  moving: 120,
  active: 140,
  high: 160,
};

function clamp(v, lo, hi) {
  return Math.min(hi, Math.max(lo, v));
}

function poseFromPosture(posture) {
  if (typeof posture !== 'string') return null;
  return POSTURE_TO_POSE[posture.toLowerCase()] || null;
}

function motionScore(data) {
  const level = data?.classification?.motion_level;
  if (typeof level === 'string') {
    const score = MOTION_LEVEL_SCORE[level.toLowerCase()];
    if (score !== undefined) return score;
  }
  const power = data?.features?.motion_band_power;
  return Number.isFinite(power) ? clamp(power * 100, 0, 200) : 0;
}

/** Scene [x, z] from the room-meter location hint, centered on the node mesh. */
function positionFromLocationHint(data) {
  const hint = data?.location_hint;
  if (!Array.isArray(hint) || hint.length < 2 ||
      !Number.isFinite(hint[0]) || !Number.isFinite(hint[1])) {
    return null;
  }
  // location_hint is expressed in configured node-position coordinates —
  // recenter on the node centroid so the figure lands inside the rendered room.
  let cx = 0, cz = 0, n = 0;
  for (const node of data?.nodes || []) {
    const p = node?.position;
    if (Array.isArray(p) && Number.isFinite(p[0]) && Number.isFinite(p[1])) {
      cx += p[0];
      cz += p[1];
      n += 1;
    }
  }
  const x = hint[0] - (n ? cx / n : 0);
  const z = hint[1] - (n ? cz / n : 0);
  return [clamp(x, -ROOM_HALF_X, ROOM_HALF_X), clamp(z, -ROOM_HALF_Z, ROOM_HALF_Z)];
}

/** Scene [x, z] from a bbox center on the 640x480 detection canvas. */
function positionFromBbox(bbox) {
  if (!bbox || !Number.isFinite(bbox.x) || !Number.isFinite(bbox.y)) return null;
  const cx = bbox.x + (Number.isFinite(bbox.width) ? bbox.width : 0) / 2;
  const cy = bbox.y + (Number.isFinite(bbox.height) ? bbox.height : 0) / 2;
  const x = (cx / CANVAS_W - 0.5) * ROOM_HALF_X * 2;
  const z = (cy / CANVAS_H - 0.5) * ROOM_HALF_Z * 2;
  return [clamp(x, -ROOM_HALF_X, ROOM_HALF_X), clamp(z, -ROOM_HALF_Z, ROOM_HALF_Z)];
}

/**
 * COCO-17 keypoints in scene coordinates: skeleton shape from the model
 * output, placement from the person position (hips over [x, z], lowest joint
 * on the floor). Accepts either [[x,y,z,conf] x17] tuples (top-level
 * pose_keypoints) or [{x,y,z,confidence} x17] objects (PersonDetection).
 * Returns null when absent or too low-confidence so callers fall back to the
 * synthetic pose system.
 */
function keypointsToScene(rawKps, x, z) {
  if (!Array.isArray(rawKps) || rawKps.length < 17) return null;
  const kps = [];
  let confSum = 0;
  for (let i = 0; i < 17; i++) {
    const k = rawKps[i];
    let kx, ky, kz, conf;
    if (Array.isArray(k)) {
      [kx, ky, kz, conf] = k;
    } else if (k && typeof k === 'object') {
      kx = k.x; ky = k.y; kz = k.z; conf = k.confidence;
    }
    if (!Number.isFinite(kx) || !Number.isFinite(ky) || !Number.isFinite(kz)) return null;
    confSum += Number.isFinite(conf) ? conf : 0;
    kps.push([kx, ky, kz]);
  }
  if (confSum / 17 < 0.15) return null; // model emitted noise — use synthetic pose
  const hipX = (kps[11][0] + kps[12][0]) / 2;
  const hipZ = (kps[11][2] + kps[12][2]) / 2;
  let minY = Infinity;
  for (const kp of kps) minY = Math.min(minY, kp[1]);
  for (const kp of kps) {
    kp[0] += x - hipX;
    kp[1] -= minY;
    kp[2] += z - hipZ;
  }
  return kps;
}

/**
 * Normalize a live "sensing_update" frame. Returns a shallow-copied frame
 * whose persons carry the demo shape the renderers read:
 * { id, position:[x,y,z], motion_score, pose, facing, keypoints17? }.
 * Demo frames (source === 'demo') pass through untouched.
 */
export function normalizeLiveFrame(raw) {
  if (!raw || raw.source === 'demo') return raw;

  const ms = motionScore(raw);
  const pose = poseFromPosture(raw.posture) || (ms > 40 ? 'walking' : 'standing');
  const hintPos = positionFromLocationHint(raw);

  const persons = [];
  const rawPersons = Array.isArray(raw.persons) ? raw.persons : [];
  if (rawPersons.length > 0) {
    rawPersons.forEach((p, i) => {
      // Primary person prefers the energy-weighted location hint; everyone
      // falls back to their bbox center, then the hint, then room center.
      const pos2 = (i === 0 ? hintPos : null) || positionFromBbox(p?.bbox) || hintPos || [0, 0];
      const position = [pos2[0], 0, pos2[1]];
      const kpSource = (Array.isArray(p?.keypoints) && p.keypoints.length >= 17)
        ? p.keypoints
        : (i === 0 ? raw.pose_keypoints : null);
      persons.push({
        id: p?.id ?? i,
        position,
        motion_score: ms,
        pose,
        facing: 0,
        confidence: p?.confidence,
        zone: p?.zone,
        keypoints17: keypointsToScene(kpSource, position[0], position[2]),
      });
    });
  } else if (raw.classification?.presence) {
    // Presence without person detections — synthesize a primary person so the
    // scene still shows a figure at the estimated location.
    const pos2 = hintPos || [0, 0];
    persons.push({
      id: 0,
      position: [pos2[0], 0, pos2[1]],
      motion_score: ms,
      pose,
      facing: 0,
      keypoints17: keypointsToScene(raw.pose_keypoints, pos2[0], pos2[1]),
    });
  }

  return { ...raw, persons };
}
