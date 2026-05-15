/**
 * RealisticFigures - Modern 3D human visualization with GLTF models
 * 
 * Features:
 * - Loads realistic 3D human models (.glb/.gltf)
 * - Supports light and dark themes
 * - Smooth pose animation via skeleton manipulation
 * - Fallback to enhanced wireframe if model fails
 */
import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';

// COCO 17-keypoint skeleton connections
export const SKELETON_CONNECTIONS = [
  [0, 1], [0, 2], [1, 3], [2, 4],  // head
  [5, 6], [5, 7], [7, 9], [6, 8], [8, 10],  // arms
  [5, 11], [6, 12], [11, 12],  // shoulders
  [11, 13], [13, 15], [12, 14], [14, 16],  // legs
];

// Light theme colors
const LIGHT_THEME = {
  background: 0xf5f5f7,
  fog: 0xf5f5f7,
  grid: 0xd0d0d0,
  floor: 0xe8e8e8,
  wireColor: 0x2d5a27,
  jointColor: 0x1a7a3a,
  aura: 0x40ff90,
};

// Dark theme colors  
const DARK_THEME = {
  background: 0x080c14,
  fog: 0x080c14,
  grid: 0x1a4830,
  floor: 0x0a0f14,
  wireColor: 0x00d878,
  jointColor: 0xff4060,
  aura: 0x40ff90,
};

export class RealisticFigures {
  constructor(scene, options = {}) {
    this.scene = scene;
    this.options = {
      theme: options.theme || 'dark',
      modelUrl: options.modelUrl || null,
      maxFigures: options.maxFigures || 4,
      scale: options.scale || 1.0,
    };
    
    this.figures = [];
    this.loader = new GLTFLoader();
    this.model = null;
    this.theme = { ...DARK_THEME };
    
    this._init();
  }

  _init() {
    this._setTheme(this.options.theme);
    this._loadModel();
  }

  _setTheme(themeName) {
    this.options.theme = themeName;
    if (themeName === 'light') {
      this.theme = { ...LIGHT_THEME };
    } else {
      this.theme = { ...DARK_THEME };
    }
    this._updateColors();
  }

  _updateColors() {
    // Update scene background
    this.scene.background = new THREE.Color(this.theme.background);
    this.scene.fog = new THREE.FogExp2(this.theme.fog, 0.008);
  }

  async _loadModel() {
    if (!this.options.modelUrl) return;
    
    try {
      const gltf = await new Promise((resolve, reject) => {
        this.loader.load(
          this.options.modelUrl,
          resolve,
          (progress) => console.log('[RealisticFigures] Loading:', (progress.loaded / progress.total * 100).toFixed(0) + '%'),
          reject
        );
      });
      
      this.model = gltf.scene;
      this.model.scale.setScalar(this.options.scale);
      this.model.traverse((child) => {
        if (child.isMesh) {
          child.castShadow = true;
          child.receiveShadow = true;
          // Enhance materials
          if (child.material) {
            child.material.metalness = 0.3;
            child.material.roughness = 0.6;
          }
        }
      });
      
      console.log('[RealisticFigures] Model loaded successfully');
      
    } catch (err) {
      console.warn('[RealisticFigures] Model load failed, using wireframe fallback:', err.message);
    }
  }

  setTheme(themeName) {
    this._setTheme(themeName);
  }

  setModelUrl(url) {
    this.options.modelUrl = url;
    this._loadModel();
  }

  // Update figure poses from keypoints
  updateFigure(index, keypoints, confidence) {
    while (this.figures.length <= index) {
      this._createFigure();
    }
    
    const figure = this.figures[index];
    if (!figure) return;

    // Show/hide based on confidence
    figure.group.visible = confidence > 0.3;
    if (!figure.group.visible) return;

    // Update joint positions
    const joints = figure.joints;
    for (let i = 0; i < Math.min(keypoints.length, 17); i++) {
      const kp = keypoints[i];
      if (kp && joints[i]) {
        // Smooth interpolation
        joints[i].position.lerp(
          new THREE.Vector3(kp.x, kp.y, kp.z),
          0.3
        );
      }
    }

    // Update bones
    this._updateBones(figure);
  }

  _updateBones(figure) {
    const joints = figure.joints;
    const bones = figure.bones;
    
    for (const bone of bones) {
      const jointA = joints[bone.a];
      const jointB = joints[bone.b];
      if (jointA && jointB) {
        const posA = jointA.position;
        const posB = jointB.position;
        
        // Position at midpoint
        bone.mesh.position.set(
          (posA.x + posB.x) / 2,
          (posA.y + posB.y) / 2,
          (posA.z + posB.z) / 2
        );
        
        // Orient toward target
        bone.mesh.lookAt(posB);
        bone.mesh.rotateX(Math.PI / 2);
        
        // Scale to fit
        const dist = posA.distanceTo(posB);
        bone.mesh.scale.z = dist;
        bone.mesh.visible = dist > 0.01;
      }
    }
  }

  _createFigure() {
    const group = new THREE.Group();
    group.visible = false;
    this.scene.add(group);

    // Create joints
    const joints = [];
    const jointSize = 0.04 * this.options.scale;
    
    for (let i = 0; i < 17; i++) {
      const geo = new THREE.SphereGeometry(jointSize, 12, 12);
      const mat = new THREE.MeshStandardMaterial({
        color: this.theme.jointColor,
        emissive: this.theme.jointColor,
        emissiveIntensity: 0.3,
        metalness: 0.5,
        roughness: 0.3,
      });
      const sphere = new THREE.Mesh(geo, mat);
      sphere.castShadow = true;
      group.add(sphere);
      joints.push(sphere);
    }

    // Create bones
    const bones = [];
    for (const [a, b] of SKELETON_CONNECTIONS) {
      const geo = new THREE.CylinderGeometry(0.015, 0.015, 1, 8);
      geo.translate(0, 0.5, 0);
      geo.rotateX(Math.PI / 2);
      
      const mat = new THREE.MeshStandardMaterial({
        color: this.theme.wireColor,
        emissive: this.theme.wireColor,
        emissiveIntensity: 0.2,
        metalness: 0.6,
        roughness: 0.3,
      });
      const mesh = new THREE.Mesh(geo, mat);
      mesh.castShadow = true;
      group.add(mesh);
      bones.push({ mesh, a, b });
    }

    // Create body mesh (if model loaded)
    let bodyMesh = null;
    if (this.model) {
      bodyMesh = this.model.clone();
      bodyMesh.visible = true;
      group.add(bodyMesh);
    }

    const figure = { group, joints, bones, bodyMesh };
    this.figures.push(figure);
    return figure;
  }

  clear() {
    for (const figure of this.figures) {
      this.scene.remove(figure.group);
      figure.group.traverse((child) => {
        if (child.geometry) child.geometry.dispose();
        if (child.material) child.material.dispose();
      });
    }
    this.figures = [];
  }

  dispose() {
    this.clear();
  }
}