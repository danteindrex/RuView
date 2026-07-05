/**
 * NodePlacement — drag-to-place 3D markers for sensing nodes (ESP32 / Pi).
 *
 * One marker per known node (GET /api/v1/nodes + /api/v1/config/node-positions,
 * polled every 5 s). Markers live in room-meter coordinates and are mapped to
 * scene units with the SAME convention live-data-mapper.js uses for the
 * location_hint: scene = meters recentered on the centroid of the live frame's
 * node positions. Dragging a marker PUTs {"positions": {id: [x,y,z]},
 * "merge": true} back to the sensing server, which immediately improves
 * person-location tracking.
 *
 * Placement mode: HUD "PLACE NODES" button (or ESC to exit). While dragging,
 * orbit controls are suspended; plain drag moves on the floor plane, holding
 * Shift adjusts height. Every marker continuously glows with its node's live
 * activity (node_features motion power, falling back to per-node RSSI
 * variance) so walking next to a physical device lights its marker.
 */
import * as THREE from 'three';
import { ROOM_HALF_X, ROOM_HALF_Z, nodeCentroid } from './live-data-mapper.js';

const MAX_HEIGHT = 3.5;
const POLL_INTERVAL_MS = 5000;
const SAVE_DEBOUNCE_MS = 300;
const RSSI_HISTORY = 40;

const COLOR_ESP32 = 0x2090ff;   // blue accent — ESP32 legacy nodes
const COLOR_PI = 0xd0355a;      // raspberry accent — Pi / nexmon nodes
const COLOR_SAVED = 0x3eff8a;

function clamp(v, lo, hi) {
  return Math.min(hi, Math.max(lo, v));
}

/** Radial-gradient halo texture shared by all markers. */
function makeHaloTexture() {
  const c = document.createElement('canvas');
  c.width = c.height = 64;
  const ctx = c.getContext('2d');
  const g = ctx.createRadialGradient(32, 32, 2, 32, 32, 32);
  g.addColorStop(0, 'rgba(255,255,255,0.9)');
  g.addColorStop(0.35, 'rgba(255,255,255,0.35)');
  g.addColorStop(1, 'rgba(255,255,255,0)');
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, 64, 64);
  return new THREE.CanvasTexture(c);
}

/** Canvas text sprite (node-id label / "PLACE ME" hint). */
function makeLabelSprite(text, cssColor, worldWidth) {
  const c = document.createElement('canvas');
  c.width = 256;
  c.height = 64;
  const ctx = c.getContext('2d');
  ctx.font = 'bold 40px "JetBrains Mono", monospace';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.shadowColor = cssColor;
  ctx.shadowBlur = 10;
  ctx.fillStyle = cssColor;
  ctx.fillText(text, 128, 34);
  const tex = new THREE.CanvasTexture(c);
  const mat = new THREE.SpriteMaterial({
    map: tex, transparent: true, depthWrite: false,
  });
  const sprite = new THREE.Sprite(mat);
  sprite.scale.set(worldWidth, worldWidth * 0.25, 1);
  return sprite;
}

export class NodePlacement {
  constructor({ scene, camera, canvas, controls, root = document, getApiBase }) {
    this._scene = scene;
    this._camera = camera;
    this._canvas = canvas;
    this._controls = controls;
    this._root = root && root.querySelector ? root : document;
    this._getApiBase = getApiBase || (() => null);
    this.onModeChanged = null;

    this._mode = false;
    this._markers = new Map();       // key (string node id) -> marker record
    this._origin = { x: 0, z: 0 };   // meters->scene recentering (mapper convention)
    this._liveNodes = null;
    this._lastTick = null;
    this._drag = null;
    this._fetching = false;
    this._disposed = false;

    this._group = new THREE.Group();
    this._group.name = 'node-placement';
    this._scene.add(this._group);

    this._raycaster = new THREE.Raycaster();
    this._ndc = new THREE.Vector2();
    this._scratchPlane = new THREE.Plane();
    this._scratchVec = new THREE.Vector3();
    this._haloTex = makeHaloTexture();
    this._toastTimer = null;

    this._onDown = (e) => this._pointerDown(e);
    this._onMove = (e) => this._pointerMove(e);
    this._onUp = () => this._endDrag(true);
    this._onKey = (e) => {
      if (e.key === 'Escape' && this._mode) this.setMode(false);
    };
    // Capture on the canvas's parent so a marker grab stops the event before
    // OrbitControls (registered directly on the canvas) ever sees it.
    this._captureHost = this._canvas?.parentElement || document.body;
    this._captureHost.addEventListener('pointerdown', this._onDown, true);
    window.addEventListener('keydown', this._onKey);

    this._pollTimer = setInterval(() => this._poll(), POLL_INTERVAL_MS);
    this._poll();
  }

  // ---- Placement mode ----

  toggleMode() {
    this.setMode(!this._mode);
    return this._mode;
  }

  setMode(on) {
    on = !!on;
    if (this._mode === on) return;
    this._mode = on;
    if (!on && this._drag) this._endDrag(false);
    if (on) {
      this._toast('DRAG A NODE MARKER — SHIFT = HEIGHT, ESC = EXIT', false, 4000);
      this._poll();
    }
    if (this.onModeChanged) this.onModeChanged(this._mode);
  }

  get modeActive() {
    return this._mode;
  }

  // ---- Server state ----

  async _poll() {
    const base = this._getApiBase();
    if (!base || this._fetching || this._disposed) return;
    this._fetching = true;
    try {
      const [nodesRes, posRes] = await Promise.all([
        fetch(`${base}/api/v1/nodes`).then((r) => (r.ok ? r.json() : null)).catch(() => null),
        fetch(`${base}/api/v1/config/node-positions`).then((r) => (r.ok ? r.json() : null)).catch(() => null),
      ]);
      if (this._disposed) return;
      if (nodesRes || posRes) {
        this._applyServerState(nodesRes?.nodes || [], posRes?.positions || {});
      }
    } finally {
      this._fetching = false;
    }
  }

  _applyServerState(nodes, positions) {
    const seen = new Set();
    const upsert = (key, info) => {
      seen.add(key);
      let m = this._markers.get(key);
      if (m && m.origin !== info.origin && info.origin) {
        this._removeMarker(m);
        m = null;
      }
      if (!m) {
        m = this._buildMarker(key, info.origin);
        this._markers.set(key, m);
      }
      m.status = info.status;
      // Never clobber a position the user is editing right now.
      if (!m.dragging && !m.saveTimer) {
        const p = info.position;
        if (Array.isArray(p) && p.length >= 3 && p.every(Number.isFinite)) {
          m.meters = [p[0], p[1], p[2]];
          m.savedMeters = m.meters.slice();
          m.parked = false;
        } else if (!m.meters) {
          m.parked = true;
        }
      }
    };

    for (const n of nodes) {
      if (n?.node_id === undefined || n.node_id === null) continue;
      upsert(String(n.node_id), {
        status: n.status || 'active',
        origin: n.origin || null,
        position: n.position,
      });
    }
    for (const [key, pos] of Object.entries(positions)) {
      if (seen.has(key)) continue;
      upsert(key, { status: 'offline', origin: null, position: pos });
    }
    for (const [key, m] of [...this._markers]) {
      if (!seen.has(key) && !m.dragging && !m.saveTimer) {
        this._removeMarker(m);
        this._markers.delete(key);
      }
    }
    this._computeOrigin();
    this._refreshTargets();
  }

  // ---- Markers ----

  _buildMarker(key, origin) {
    const isPi = origin === 'nexmon';
    const accent = isPi ? COLOR_PI : COLOR_ESP32;
    const accentCss = isPi ? '#d0355a' : '#2090ff';
    const group = new THREE.Group();
    group.userData.nodeKey = key;

    const bodyMat = new THREE.MeshStandardMaterial({
      color: 0x222a34, roughness: 0.35, metalness: 0.5,
      emissive: accent, emissiveIntensity: 0.15,
    });
    let body;
    if (isPi) {
      // Pi: flat board silhouette with a GPIO header bar, no antenna.
      body = new THREE.Mesh(new THREE.BoxGeometry(0.26, 0.05, 0.18), bodyMat);
      body.position.y = 0.025;
      const header = new THREE.Mesh(
        new THREE.BoxGeometry(0.2, 0.03, 0.04),
        new THREE.MeshStandardMaterial({ color: 0x888890, roughness: 0.4, metalness: 0.7 })
      );
      header.position.set(0, 0.06, -0.06);
      group.add(header);
    } else {
      // ESP32: taller "device" with an antenna stub.
      body = new THREE.Mesh(new THREE.BoxGeometry(0.2, 0.1, 0.14), bodyMat);
      body.position.y = 0.05;
      const antenna = new THREE.Mesh(
        new THREE.CylinderGeometry(0.008, 0.008, 0.16),
        new THREE.MeshStandardMaterial({ color: 0x606068, roughness: 0.3, metalness: 0.6 })
      );
      antenna.position.set(0.07, 0.17, 0);
      group.add(antenna);
    }
    group.add(body);

    const led = new THREE.Mesh(
      new THREE.SphereGeometry(0.02),
      new THREE.MeshBasicMaterial({ color: accent })
    );
    led.position.set(-0.07, isPi ? 0.06 : 0.11, 0.06);
    group.add(led);

    const halo = new THREE.Sprite(new THREE.SpriteMaterial({
      map: this._haloTex, color: accent, transparent: true, opacity: 0.2,
      blending: THREE.AdditiveBlending, depthWrite: false,
    }));
    halo.scale.set(0.9, 0.9, 1);
    halo.position.y = 0.1;
    group.add(halo);

    const label = makeLabelSprite(`N${key}`, accentCss, 0.8);
    label.position.y = 0.5;
    group.add(label);

    const hintLabel = makeLabelSprite('PLACE ME', '#ffb020', 1.0);
    hintLabel.position.y = 0.78;
    hintLabel.visible = false;
    group.add(hintLabel);

    // Floor ring + stem so an elevated marker keeps a readable XZ anchor.
    const ring = new THREE.Mesh(
      new THREE.RingGeometry(0.16, 0.2, 24),
      new THREE.MeshBasicMaterial({
        color: accent, transparent: true, opacity: 0.35, side: THREE.DoubleSide,
        depthWrite: false,
      })
    );
    ring.rotation.x = -Math.PI / 2;
    group.add(ring);
    const stemGeo = new THREE.BufferGeometry().setFromPoints([
      new THREE.Vector3(0, 0, 0), new THREE.Vector3(0, 0, 0),
    ]);
    const stem = new THREE.Line(stemGeo, new THREE.LineBasicMaterial({
      color: accent, transparent: true, opacity: 0.3,
    }));
    group.add(stem);

    this._group.add(group);
    return {
      key, origin: origin || null, group, halo, led, label, hintLabel, ring, stem,
      status: 'active', meters: null, savedMeters: null, parked: true,
      dragging: false, saveTimer: null, flash: 0,
      target: new THREE.Vector3(0, 0, ROOM_HALF_Z + 0.2),
      activity: 0, actMax: 0, rssiHist: [],
    };
  }

  _removeMarker(m) {
    this._group.remove(m.group);
    m.group.traverse((obj) => {
      obj.geometry?.dispose?.();
      if (obj.material && obj.material.map !== this._haloTex) obj.material.map?.dispose?.();
      obj.material?.dispose?.();
    });
    if (m.saveTimer) clearTimeout(m.saveTimer);
  }

  /** Node centroid used to recenter meters into the scene — the SAME helper
   *  positionFromLocationHint() in live-data-mapper.js uses on update.nodes. */
  _computeOrigin() {
    let c = nodeCentroid(this._liveNodes);
    if (!c.count) {
      // No live frame yet — approximate with the configured positions.
      c = nodeCentroid(
        [...this._markers.values()].map((m) => (m.meters ? { position: m.meters } : null))
      );
    }
    this._origin = { x: c.x, z: c.z };
  }

  _sceneFromMeters(meters) {
    return new THREE.Vector3(
      clamp(meters[0] - this._origin.x, -ROOM_HALF_X - 0.3, ROOM_HALF_X + 0.3),
      clamp(meters[2] || 0, 0, MAX_HEIGHT),
      clamp(meters[1] - this._origin.z, -ROOM_HALF_Z - 0.3, ROOM_HALF_Z + 0.3)
    );
  }

  _metersFromScene(pos) {
    return [pos.x + this._origin.x, pos.z + this._origin.z, pos.y];
  }

  _refreshTargets() {
    const parked = [...this._markers.keys()]
      .filter((k) => this._markers.get(k).parked)
      .sort((a, b) => Number(a) - Number(b));
    for (const [key, m] of this._markers) {
      if (m.dragging) continue;
      if (m.parked) {
        // Parking row along the front room edge for unplaced nodes.
        const i = parked.indexOf(key);
        m.target.set(-4.5 + i * 1.1, 0, ROOM_HALF_Z + 0.2);
      } else if (m.meters) {
        m.target.copy(this._sceneFromMeters(m.meters));
      }
      m.hintLabel.visible = m.parked;
    }
  }

  // ---- Per-frame update (called from Observatory._animate) ----

  update(data, dt, elapsed) {
    if (this._disposed) return;
    if (data && data.source !== 'demo' &&
        Array.isArray(data.nodes) && data.nodes.length &&
        data.tick !== this._lastTick) {
      this._lastTick = data.tick;
      this._liveNodes = data.nodes;
      this._computeOrigin();
      this._refreshTargets();
      this._ingestActivity(data);
    }

    const lerp = 1 - Math.exp(-(dt || 0.016) * 8);
    for (const m of this._markers.values()) {
      if (!m.dragging) m.group.position.lerp(m.target, lerp);

      // Floor ring / stem counter-offset so they stay on the floor.
      const y = m.group.position.y;
      m.ring.position.y = -y + 0.012;
      const sp = m.stem.geometry.attributes.position;
      sp.setY(0, -y + 0.02);
      sp.needsUpdate = true;
      m.stem.visible = y > 0.08;

      // Live glow: base level (subdued outside placement mode) + activity.
      const offline = m.status !== 'active';
      const act = offline ? 0 : m.activity;
      m.activity = act * Math.exp(-(dt || 0.016) * 0.4); // slow decay between frames
      let glow = (this._mode ? 0.3 : 0.14) + act * 0.85;
      glow *= 0.75 + 0.25 * Math.sin(elapsed * (2 + 8 * act));
      if (m.parked) glow = Math.max(glow, 0.3 + 0.25 * Math.sin(elapsed * 3));
      if (m.flash > 0) {
        m.flash -= dt || 0.016;
        m.halo.material.color.setHex(COLOR_SAVED);
        glow = Math.max(glow, m.flash);
      } else {
        m.halo.material.color.setHex(m.origin === 'nexmon' ? COLOR_PI : COLOR_ESP32);
      }
      m.halo.material.opacity = clamp(offline ? 0.05 : glow, 0, 1);
      m.led.material.color.setHex(offline ? 0x555555
        : (m.flash > 0 ? COLOR_SAVED : (m.origin === 'nexmon' ? COLOR_PI : COLOR_ESP32)));
      m.label.material.opacity = offline ? 0.35 : 1;
      m.hintLabel.material.opacity = 0.55 + 0.45 * Math.sin(elapsed * 3);
    }
  }

  /** Activity from per-node motion (node_features) or RSSI variance fallback. */
  _ingestActivity(data) {
    const feats = new Map();
    if (Array.isArray(data.node_features)) {
      for (const f of data.node_features) {
        if (f && f.node_id !== undefined && !f.stale) {
          feats.set(String(f.node_id), f.features?.motion_band_power ?? 0);
        }
      }
    }
    for (const node of this._liveNodes) {
      const key = String(node?.node_id);
      const m = this._markers.get(key);
      if (!m) continue;
      let v;
      let floor; // absolute scale below which the node counts as quiet
      if (feats.has(key)) {
        v = feats.get(key);
        floor = 0.05; // motion_band_power scale
      } else {
        // Per-node RSSI variance over the recent history.
        if (Number.isFinite(node.rssi_dbm)) {
          m.rssiHist.push(node.rssi_dbm);
          if (m.rssiHist.length > RSSI_HISTORY) m.rssiHist.shift();
        }
        if (m.rssiHist.length < 8) continue;
        const n = m.rssiHist.length;
        const mean = m.rssiHist.reduce((a, b) => a + b, 0) / n;
        v = m.rssiHist.reduce((a, b) => a + (b - mean) * (b - mean), 0) / n;
        floor = 2.0; // dB^2 — below this the link is considered quiet
      }
      // Normalize against a slowly-decaying per-node peak (with an absolute
      // floor) so per-hardware scale differences cancel without hand tuning.
      m.actMax = Math.max(m.actMax * 0.995, v);
      m.activity = clamp(v / Math.max(m.actMax, floor), 0, 1);
    }
  }

  // ---- Dragging ----

  _pointerDown(e) {
    if (!this._mode || this._disposed || e.button !== 0 || e.target !== this._canvas) return;
    const m = this._pickMarker(e);
    if (!m) return; // empty space — let OrbitControls have it
    e.preventDefault();
    e.stopPropagation();
    const controlsWereEnabled = this._controls.enabled;
    this._controls.enabled = false;
    const hit = this._planeHit(e, m, e.shiftKey);
    this._drag = {
      marker: m,
      vertical: e.shiftKey,
      offset: hit ? new THREE.Vector3().subVectors(m.group.position, hit) : new THREE.Vector3(),
      moved: false,
      controlsWereEnabled,
    };
    m.dragging = true;
    window.addEventListener('pointermove', this._onMove);
    window.addEventListener('pointerup', this._onUp);
  }

  _pointerMove(e) {
    const drag = this._drag;
    if (!drag) return;
    const m = drag.marker;
    const hit = this._planeHit(e, m, e.shiftKey);
    if (!hit) return;
    if (e.shiftKey !== drag.vertical) {
      // Constraint changed mid-drag — re-anchor so the marker doesn't jump.
      drag.vertical = e.shiftKey;
      drag.offset.subVectors(m.group.position, hit);
      return;
    }
    const t = hit.add(drag.offset);
    if (drag.vertical) {
      m.group.position.y = clamp(t.y, 0, MAX_HEIGHT);
    } else {
      m.group.position.x = clamp(t.x, -ROOM_HALF_X - 0.3, ROOM_HALF_X + 0.3);
      m.group.position.z = clamp(t.z, -ROOM_HALF_Z - 0.3, ROOM_HALF_Z + 0.3);
    }
    m.target.copy(m.group.position);
    drag.moved = true;
  }

  _endDrag(commit) {
    const drag = this._drag;
    window.removeEventListener('pointermove', this._onMove);
    window.removeEventListener('pointerup', this._onUp);
    this._drag = null;
    if (!drag) return;
    this._controls.enabled = drag.controlsWereEnabled;
    const m = drag.marker;
    m.dragging = false;
    if (commit && drag.moved) {
      m.parked = false;
      m.hintLabel.visible = false;
      m.meters = this._metersFromScene(m.group.position);
      this._scheduleSave(m);
    } else {
      this._refreshTargets();
    }
  }

  /** Point the shared raycaster at the event's canvas position. */
  _aimRay(e) {
    const rect = this._canvas.getBoundingClientRect();
    this._ndc.set(
      ((e.clientX - rect.left) / rect.width) * 2 - 1,
      -((e.clientY - rect.top) / rect.height) * 2 + 1
    );
    this._raycaster.setFromCamera(this._ndc, this._camera);
  }

  /** Ray/plane intersection: floor plane, or a camera-facing vertical plane
   *  through the marker when adjusting height. */
  _planeHit(e, m, vertical) {
    this._aimRay(e);
    const plane = this._scratchPlane;
    if (vertical) {
      const n = this._scratchVec;
      this._camera.getWorldDirection(n);
      n.y = 0;
      if (n.lengthSq() < 1e-6) n.set(0, 0, 1);
      n.normalize();
      plane.setFromNormalAndCoplanarPoint(n, m.group.position);
    } else {
      plane.normal.set(0, 1, 0);
      plane.constant = 0;
    }
    const out = new THREE.Vector3();
    return this._raycaster.ray.intersectPlane(plane, out) ? out : null;
  }

  _pickMarker(e) {
    this._aimRay(e);
    const hits = this._raycaster.intersectObjects(this._group.children, true);
    for (const h of hits) {
      let obj = h.object;
      while (obj && !obj.userData.nodeKey) obj = obj.parent;
      if (obj) return this._markers.get(obj.userData.nodeKey) || null;
    }
    return null;
  }

  // ---- Persistence ----

  _scheduleSave(m) {
    if (m.saveTimer) clearTimeout(m.saveTimer);
    m.saveTimer = setTimeout(() => this._saveMarker(m), SAVE_DEBOUNCE_MS);
  }

  _saveMarker(m) {
    m.saveTimer = null;
    const base = this._getApiBase();
    const meters = m.meters ? m.meters.slice() : null;
    if (!base || !meters) {
      this._revert(m);
      this._toast('NO SENSING SERVER — POSITION NOT SAVED', true);
      return;
    }
    fetch(`${base}/api/v1/config/node-positions`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ positions: { [m.key]: meters }, merge: true }),
    })
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((res) => {
        if (!res || res.success !== true) throw new Error(res?.error || 'rejected');
        m.savedMeters = meters;
        m.flash = 1.0;
        this._computeOrigin();
        this._refreshTargets();
        this._toast(`NODE ${m.key} POSITION SAVED`);
      })
      .catch(() => {
        this._revert(m);
        this._toast(`NODE ${m.key} SAVE FAILED — REVERTED`, true);
      });
  }

  _revert(m) {
    if (m.savedMeters) {
      m.meters = m.savedMeters.slice();
      m.parked = false;
    } else {
      m.meters = null;
      m.parked = true;
    }
    this._refreshTargets();
  }

  // ---- Toast ----

  _toast(msg, isError = false, ms = 1800) {
    const hud = this._root.querySelector('#hud');
    if (!hud) return;
    let el = hud.querySelector('#node-toast');
    if (!el) {
      el = document.createElement('div');
      el.id = 'node-toast';
      hud.appendChild(el);
    }
    el.textContent = msg;
    el.classList.toggle('error', isError);
    el.classList.add('show');
    if (this._toastTimer) clearTimeout(this._toastTimer);
    this._toastTimer = setTimeout(() => el.classList.remove('show'), ms);
  }

  // ---- Teardown (ui-v2 unmounts the observatory) ----

  dispose() {
    if (this._disposed) return;
    this._disposed = true;
    if (this._drag) this._endDrag(false);
    clearInterval(this._pollTimer);
    if (this._toastTimer) clearTimeout(this._toastTimer);
    this._captureHost.removeEventListener('pointerdown', this._onDown, true);
    window.removeEventListener('keydown', this._onKey);
    window.removeEventListener('pointermove', this._onMove);
    window.removeEventListener('pointerup', this._onUp);
    for (const m of this._markers.values()) this._removeMarker(m);
    this._markers.clear();
    this._scene.remove(this._group);
    this._haloTex.dispose();
  }
}
