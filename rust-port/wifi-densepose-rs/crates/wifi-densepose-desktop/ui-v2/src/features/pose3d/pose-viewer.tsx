import { useEffect, useRef } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import type { PoseLandmark } from "@/types";

const POSE_CONNECTIONS: Array<[number, number]> = [
  [0, 1], [1, 2], [2, 3], [3, 7],
  [0, 4], [4, 5], [5, 6], [6, 8],
  [9, 10],
  [11, 12], [11, 13], [13, 15], [15, 17], [15, 19], [15, 21], [17, 19],
  [12, 14], [14, 16], [16, 18], [16, 20], [16, 22], [18, 20],
  [11, 23], [12, 24], [23, 24],
  [23, 25], [25, 27], [27, 29], [29, 31],
  [24, 26], [26, 28], [28, 30], [30, 32],
  [27, 31], [28, 32],
];

interface PoseViewerProps {
  landmarks: PoseLandmark[] | null;
}

type ViewerState = {
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  renderer: THREE.WebGLRenderer;
  controls: OrbitControls;
  poseGroup: THREE.Group;
  resizeObserver: ResizeObserver;
  animationHandle: number;
};

export function PoseViewer({ landmarks }: PoseViewerProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const stateRef = useRef<ViewerState | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const scene = new THREE.Scene();
    scene.background = new THREE.Color("#08131f");
    scene.fog = new THREE.Fog("#08131f", 3.5, 9);

    const camera = new THREE.PerspectiveCamera(52, 1, 0.01, 100);
    camera.position.set(1.4, 1.2, 2.2);

    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    container.appendChild(renderer.domElement);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    controls.minDistance = 0.6;
    controls.maxDistance = 10;
    controls.target.set(0, 0.6, 0);
    controls.update();

    const ambient = new THREE.AmbientLight("#74d6ff", 0.55);
    const key = new THREE.DirectionalLight("#d2f4ff", 1.3);
    key.position.set(2, 2.8, 1.6);
    const rim = new THREE.DirectionalLight("#64ffd2", 0.8);
    rim.position.set(-2, 1.4, -2.2);
    scene.add(ambient, key, rim);

    const grid = new THREE.GridHelper(4, 16, "#1a8fb3", "#153347");
    grid.position.y = -0.9;
    scene.add(grid);

    const axes = new THREE.AxesHelper(0.6);
    scene.add(axes);

    const poseGroup = new THREE.Group();
    scene.add(poseGroup);

    const resize = () => {
      const width = container.clientWidth;
      const height = container.clientHeight;
      if (width === 0 || height === 0) return;
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
      renderer.setSize(width, height, false);
    };

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
    resize();

    const animate = () => {
      controls.update();
      renderer.render(scene, camera);
      const animationHandle = requestAnimationFrame(animate);
      if (stateRef.current) {
        stateRef.current.animationHandle = animationHandle;
      }
    };
    animate();

    stateRef.current = {
      scene,
      camera,
      renderer,
      controls,
      poseGroup,
      resizeObserver,
      animationHandle: 0,
    };

    return () => {
      const current = stateRef.current;
      if (current) {
        cancelAnimationFrame(current.animationHandle);
        current.resizeObserver.disconnect();
        current.controls.dispose();
        current.renderer.dispose();
        current.scene.clear();
        stateRef.current = null;
      }
      renderer.domElement.remove();
    };
  }, []);

  useEffect(() => {
    const state = stateRef.current;
    if (!state) return;

    while (state.poseGroup.children.length > 0) {
      const child = state.poseGroup.children.pop();
      if (child instanceof THREE.Mesh) {
        child.geometry.dispose();
        if (Array.isArray(child.material)) {
          child.material.forEach((material) => material.dispose());
        } else {
          child.material.dispose();
        }
      } else if (child instanceof THREE.Line) {
        child.geometry.dispose();
        if (Array.isArray(child.material)) {
          child.material.forEach((material) => material.dispose());
        } else {
          child.material.dispose();
        }
      }
    }

    if (!landmarks || landmarks.length < 33) return;

    const points = landmarks.map((point) => new THREE.Vector3(point.x, -point.y, -point.z));

    for (const [from, to] of POSE_CONNECTIONS) {
      const left = landmarks[from];
      const right = landmarks[to];
      if (!left || !right) continue;

      const confidence = Math.min(left.visibility ?? 1, left.presence ?? 1, right.visibility ?? 1, right.presence ?? 1);
      if (confidence < 0.1) continue;

      const geometry = new THREE.BufferGeometry().setFromPoints([points[from], points[to]]);
      const material = new THREE.LineBasicMaterial({
        color: new THREE.Color().setHSL(0.54, 0.92, 0.58),
        transparent: true,
        opacity: Math.max(0.2, confidence),
      });
      const line = new THREE.Line(geometry, material);
      state.poseGroup.add(line);
    }

    for (let index = 0; index < points.length; index += 1) {
      const point = points[index];
      const source = landmarks[index];
      const confidence = Math.min(source.visibility ?? 1, source.presence ?? 1);
      if (confidence < 0.05) continue;

      const radius = 0.012 + Math.max(0, 0.016 * confidence);
      const geometry = new THREE.SphereGeometry(radius, 14, 14);
      const material = new THREE.MeshStandardMaterial({
        color: new THREE.Color().setHSL(0.46 + (index / points.length) * 0.14, 0.86, 0.6),
        emissive: "#0b4f5f",
        emissiveIntensity: 0.6,
        transparent: true,
        opacity: Math.max(0.25, confidence),
      });
      const dot = new THREE.Mesh(geometry, material);
      dot.position.copy(point);
      state.poseGroup.add(dot);
    }
  }, [landmarks]);

  return <div ref={containerRef} className="h-[520px] w-full overflow-hidden rounded-lg border border-border/60" />;
}

