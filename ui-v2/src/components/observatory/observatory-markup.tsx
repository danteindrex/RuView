import React from "react";

export function ObservatoryMarkup() {
  return (
    <>
      <canvas id="observatory-canvas" />

      <div id="hud">
        <div id="brand">
          <div id="brand-logo">
            <span className="pi">π</span> Wave
          </div>
          <div id="brand-tagline">WiFi DensePose Sensing Observatory</div>
        </div>

        <div id="status-bar">
          <div id="data-source-badge">
            <span className="dot dot--demo" />
            <span id="data-source-label">WAITING</span>
          </div>
          <div id="scenario-area">
            <span id="autoplay-icon" title="Auto-cycling"></span>
            <select id="scenario-quick-select" title="Change scenario" defaultValue="auto">
              <option value="auto">Auto-Cycle</option>
              <option value="empty_room">Empty Room</option>
              <option value="single_breathing">Vital Signs</option>
              <option value="two_walking">Multi-Person</option>
              <option value="fall_event">Fall Detect</option>
              <option value="sleep_monitoring">Sleep Monitor</option>
              <option value="intrusion_detect">Intrusion</option>
              <option value="gesture_control">Gesture Ctrl</option>
              <option value="crowd_occupancy">Crowd (4 ppl)</option>
              <option value="search_rescue">Search Rescue</option>
              <option value="elderly_care">Elderly Care</option>
              <option value="fitness_tracking">Fitness</option>
              <option value="security_patrol">Security Patrol</option>
            </select>
          </div>
          <div id="scenario-description" />
          <div id="fps-counter" style={{ display: "none" }}>
            60 FPS
          </div>
          <a
            id="pose-fusion-link"
            href="#"
            onClick={(event) => event.preventDefault()}
          >
            Pose Fusion
          </a>
          <button id="place-nodes-btn" title="Drag node markers to where the physical devices sit">
            PLACE NODES
          </button>
          <button id="settings-btn" title="Settings">
            ⚙
          </button>
        </div>

        <div id="panel-vitals" className="data-panel">
          <div className="panel-header">Vital Signs</div>
          <div className="vital-row">

            <div className="vital-data">
              <div className="vital-label">Heart Rate</div>
              <div className="vital-value">
                <span id="hr-value">--</span> <span className="vital-unit">BPM</span>
              </div>
              <div className="vital-bar">
                <div id="hr-bar" className="vital-bar-fill vital-bar--hr" />
              </div>
            </div>
          </div>
          <div className="vital-row">

            <div className="vital-data">
              <div className="vital-label">Respiration</div>
              <div className="vital-value">
                <span id="br-value">--</span> <span className="vital-unit">RPM</span>
              </div>
              <div className="vital-bar">
                <div id="br-bar" className="vital-bar-fill vital-bar--br" />
              </div>
            </div>
          </div>
          <div className="vital-row">

            <div className="vital-data">
              <div className="vital-label">Confidence</div>
              <div className="vital-value">
                <span id="conf-value">--</span>
                <span className="vital-unit">%</span>
              </div>
              <div className="vital-bar">
                <div id="conf-bar" className="vital-bar-fill vital-bar--conf" />
              </div>
            </div>
          </div>
        </div>

        <div id="panel-signal" className="data-panel">
          <div className="panel-header">WiFi Signal</div>
          <div className="signal-row">
            <span className="signal-label">RSSI</span>
            <span className="signal-value" id="rssi-value">
              -- dBm
            </span>
          </div>
          <div className="signal-row">
            <span className="signal-label">Variance</span>
            <span className="signal-value" id="var-value">
              --
            </span>
          </div>
          <div className="signal-row">
            <span className="signal-label">Motion</span>
            <span className="signal-value" id="motion-value">
              --
            </span>
          </div>
          <div className="signal-row">
            <span className="signal-label">Persons</span>
            <span className="signal-value" id="persons-value">
              0
            </span>
            <span id="persons-dots" className="persons-dots" />
          </div>
          <canvas id="rssi-sparkline" width={200} height={48} />

          <div className="panel-header" style={{ marginTop: 12 }}>
            Presence
          </div>
          <div id="presence-indicator" className="presence-state presence--absent">
            <span id="presence-label">ABSENT</span>
          </div>
          <div id="fall-alert" className="fall-alert" style={{ display: "none" }}>
            FALL DETECTED
          </div>
        </div>

        <div id="edge-modules-bar" />

        <div id="capabilities-bar">
          <div className="cap-item">

            <span>Human Pose Estimation</span>
          </div>
          <div className="cap-divider" />
          <div className="cap-item">

            <span>Vital Sign Monitoring</span>
          </div>
          <div className="cap-divider" />
          <div className="cap-item">

            <span>Presence Detection</span>
          </div>
        </div>

        <div id="key-hints">
          <span className="key-hint">[A] Orbit</span>
          <span className="key-hint">[D] Scenario</span>
          <span className="key-hint">[F] FPS</span>
          <span className="key-hint">[S] Settings</span>
          <span className="key-hint">[Space] Pause</span>
        </div>
      </div>

      <div id="settings-overlay" className="settings-overlay" style={{ display: "none" }}>
        <div className="settings-dialog">
          <div className="settings-header">
            <span>Settings</span>
            <button id="settings-close">&times;</button>
          </div>

          <div className="settings-tabs">
            <button className="stab active" data-stab="rendering">Rendering</button>
            <button className="stab" data-stab="wireframe">Wireframe</button>
            <button className="stab" data-stab="scene">Scene</button>
            <button className="stab" data-stab="data">Data</button>
          </div>

          <div className="stab-content active" id="stab-rendering">
            <SettingRange id="opt-bloom" label="Bloom Strength" min="0" max="3" step="0.1" value="1.0" />
            <SettingRange id="opt-bloom-radius" label="Bloom Radius" min="0" max="1" step="0.05" value="0.5" />
            <SettingRange id="opt-bloom-thresh" label="Bloom Threshold" min="0" max="1" step="0.05" value="0.25" />
            <SettingRange id="opt-exposure" label="Exposure" min="0.3" max="2" step="0.05" value="0.9" />
            <SettingRange id="opt-vignette" label="Vignette" min="0" max="1" step="0.05" value="0.5" />
            <SettingRange id="opt-grain" label="Film Grain" min="0" max="0.15" step="0.005" value="0.03" />
            <SettingRange id="opt-chromatic" label="Chromatic Aberration" min="0" max="0.008" step="0.0005" value="0.0015" />
          </div>

          <div className="stab-content" id="stab-wireframe">
            <SettingRange id="opt-bone-thick" label="Bone Thickness" min="0.005" max="0.06" step="0.002" value="0.02" />
            <SettingRange id="opt-joint-size" label="Joint Size" min="0.02" max="0.12" step="0.005" value="0.05" />
            <SettingRange id="opt-glow" label="Glow Intensity" min="0" max="2" step="0.1" value="0.8" />
            <SettingRange id="opt-trail" label="Particle Trail" min="0" max="1" step="0.05" value="0.6" />
            <SettingColor id="opt-wire-color" label="Wireframe Color" value="#00d878" />
            <SettingColor id="opt-joint-color" label="Joint Color" value="#ff4060" />
            <SettingRange id="opt-aura" label="Aura Opacity" min="0" max="0.2" step="0.01" value="0.06" />
          </div>

          <div className="stab-content" id="stab-scene">
            <SettingRange id="opt-field" label="Signal Field" min="0" max="1" step="0.05" value="0.5" />
            <SettingRange id="opt-waves" label="WiFi Waves" min="0" max="1" step="0.05" value="0.6" />
            <SettingRange id="opt-ambient" label="Room Brightness" min="0" max="1" step="0.05" value="0.4" />
            <SettingRange id="opt-reflect" label="Floor Reflection" min="0" max="1" step="0.05" value="0.3" />
            <SettingRange id="opt-fov" label="FOV" min="30" max="90" step="1" value="50" />
            <SettingRange id="opt-orbit-speed" label="Orbit Speed" min="0.02" max="0.5" step="0.02" value="0.15" />
            <SettingCheckbox id="opt-grid" label="Show Grid" defaultChecked />
            <SettingCheckbox id="opt-room" label="Show Room Boundary" defaultChecked />
          </div>

          <div className="stab-content" id="stab-data">
            <div className="setting-row">
              <span>Scenario</span>
              <select id="opt-scenario" defaultValue="auto">
                <option value="auto">Auto-Cycle (30s)</option>
                <optgroup label="Core Sensing">
                  <option value="empty_room">Empty Room</option>
                  <option value="single_breathing">Vital Signs (Breathing)</option>
                  <option value="two_walking">Multi-Person Tracking</option>
                  <option value="fall_event">Fall Detection</option>
                </optgroup>
                <optgroup label="Medical / Health">
                  <option value="sleep_monitoring">Sleep Monitoring (Apnea)</option>
                  <option value="elderly_care">Elderly Care (Gait)</option>
                  <option value="fitness_tracking">Fitness Tracking</option>
                </optgroup>
                <optgroup label="Security">
                  <option value="intrusion_detect">Intrusion Detection</option>
                  <option value="security_patrol">Security Patrol</option>
                </optgroup>
                <optgroup label="Building / Retail">
                  <option value="crowd_occupancy">Crowd Occupancy (4 ppl)</option>
                  <option value="gesture_control">Gesture Control (DTW)</option>
                </optgroup>
                <optgroup label="Disaster / Tactical">
                  <option value="search_rescue">Search &amp; Rescue (WiFi-MAT)</option>
                </optgroup>
              </select>
            </div>
            <SettingRange id="opt-cycle" label="Cycle Speed (s)" min="10" max="120" step="5" value="30" />
            <div className="setting-row">
              <span>Style Preset</span>
              <select id="opt-preset" defaultValue="custom">
                <option value="custom">Custom</option>
                <option value="foundation">Foundation (Default)</option>
                <option value="cinematic">Cinematic</option>
                <option value="minimal">Minimal / Clean</option>
                <option value="neon">Neon Glow</option>
                <option value="tactical">Tactical / Military</option>
                <option value="medical">Medical Monitor</option>
              </select>
            </div>
            <div className="setting-row">
              <span>Data Source</span>
              <select id="opt-data-source" defaultValue="ws">
                <option value="ws">Live WebSocket</option>
                <option value="demo">Demo Scenario</option>
              </select>
            </div>
            <div className="setting-row" id="ws-url-row" style={{ display: "none" }}>
              <span>WS URL</span>
              <input type="text" id="opt-ws-url" defaultValue="" placeholder="ws://localhost:3000/ws/sensing" />
            </div>
            <button id="btn-reset-camera" className="settings-btn">Reset Camera</button>
            <button id="btn-reset-settings" className="settings-btn">Reset to Defaults</button>
            <button id="btn-export-settings" className="settings-btn">Export Settings</button>
          </div>
        </div>
      </div>
    </>
  );
}

function SettingRange(props: { id: string; label: string; min: string; max: string; step: string; value: string }) {
  const valueId = `${props.id}-val`;
  return (
    <label className="setting-row">
      <span>{props.label}</span>
      <input type="range" id={props.id} min={props.min} max={props.max} step={props.step} defaultValue={props.value} />
      <span className="range-val" id={valueId}>{props.value}</span>
    </label>
  );
}

function SettingColor(props: { id: string; label: string; value: string }) {
  return (
    <label className="setting-row">
      <span>{props.label}</span>
      <input type="color" id={props.id} defaultValue={props.value} />
    </label>
  );
}

function SettingCheckbox(props: { id: string; label: string; defaultChecked?: boolean }) {
  return (
    <label className="setting-row check-row">
      <span>{props.label}</span>
      <input type="checkbox" id={props.id} defaultChecked={props.defaultChecked} />
    </label>
  );
}
