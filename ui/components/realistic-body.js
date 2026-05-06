// Enhanced Body Model Manager with GLTF Support - WiFi DensePose
// Supports loading realistic 3D models with fallback to primitive skeleton

import { GLTFLoader } from 'three';

export class RealisticBodyModelManager {
  constructor(scene) {
    this.scene = scene;
    this.group = new THREE.Group();
    this.group.name = 'body-models';
    this.scene.add(this.group);

    this.bodies = new Map(); // bodyId -> BodyModel
    this.loader = new GLTFLoader();
    this.loadedModels = new Map(); // modelUrl -> GLTF scene
    
    // Configuration
    this.config = {
      maxBodies: 10,
      modelUrl: null, // Set to a GLTF/GLB URL to use realistic models
      useFallback: true
    };
    
    // Material cache for performance
    this._materialCache = this._createMaterialCache();
  }

  _createMaterialCache() {
    return {
      // PBR-style materials for realistic look
      primary: new THREE.MeshStandardMaterial({
        color: 0x00ccff,
        emissive: 0x004466,
        emissiveIntensity: 0.5,
        metalness: 0.6,
        roughness: 0.3,
        transparent: true,
        opacity: 0.95
      }),
      secondary: new THREE.MeshStandardMaterial({
        color: 0x00ff88,
        emissive: 0x006644,
        emissiveIntensity: 0.4,
        metalness: 0.5,
        roughness: 0.4,
        transparent: true,
        opacity: 0.9
      }),
      accent: new THREE.MeshStandardMaterial({
        color: 0xff6600,
        emissive: 0xff3300,
        emissiveIntensity: 0.6,
        metalness: 0.7,
        roughness: 0.2,
        transparent: true,
        opacity: 0.9
      }),
      wireframe: new THREE.MeshBasicMaterial({
        color: 0x00ffcc,
        wireframe: true,
        transparent: true,
        opacity: 0.3
      })
    };
  }

  // Load a GLTF/GLB model for use
  loadModel(url) {
    return new Promise((resolve, reject) => {
      if (this.loadedModels.has(url)) {
        resolve(this.loadedModels.get(url));
        return;
      }
      
      this.loader.load(
        url,
        (gltf) => {
          const model = gltf.scene;
          
          // Optimize: traverse and cache materials
          model.traverse((child) => {
            if (child.isMesh) {
              child.castShadow = true;
              child.receiveShadow = true;
            }
          });
          
          this.loadedModels.set(url, model);
          console.log('[RealisticBody] Loaded model:', url);
          resolve(model);
        },
        (progress) => {
          const percent = (progress.loaded / progress.total * 100).toFixed(0);
          console.log(`[RealisticBody] Loading: ${percent}%`);
        },
        (error) => {
          console.error('[RealisticBody] Load error:', error);
          reject(error);
        }
      );
    });
  }

  // Set custom model URL
  setModelUrl(url) {
    this.config.modelUrl = url;
    if (url) {
      this.loadModel(url).catch(err => {
        console.warn('[RealisticBody] Failed to load model, using fallback');
        this.config.useFallback = true;
      });
    }
  }

  // Create a body instance (either from GLTF or fallback skeleton)
  createBody(bodyId) {
    if (this.bodies.size >= this.config.maxBodies) {
      console.warn('[RealisticBody] Max bodies reached');
      return null;
    }

    let body;
    
    if (this.config.modelUrl && this.loadedModels.has(this.config.modelUrl)) {
      body = this._createGLTFBody(bodyId);
    } else {
      body = this._createFallbackBody(bodyId);
    }

    this.bodies.set(bodyId, body);
    this.group.add(body.group);
    
    return body;
  }

  _createGLTFBody(bodyId) {
    const template = this.loadedModels.get(this.config.modelUrl);
    const model = template.clone();
    
    // Scale and position
    model.scale.setScalar(1.0);
    model.position.set(0, 0, 0);

    return {
      id: bodyId,
      group: model,
      type: 'gltf',
      mesh: model,
      update: (keypoints, confidence) => {
        // Map keypoints to model bones if available
        this._updateGLTFBody(model, keypoints, confidence);
      },
      dispose: () => {
        model.traverse((child) => {
          if (child.isMesh) {
            child.geometry?.dispose();
          }
        });
      }
    };
  }

  _updateGLTFBody(model, keypoints, confidence) {
    // If model has bones (skeleton), map COCO keypoints to bones
    model.traverse((child) => {
      if (child.isBone) {
        // Map keypoints to bone transforms
        // This is a simplified version - full implementation would need
        // bone mapping based on the specific GLTF model structure
      }
    });
    
    // Adjust visibility based on confidence
    model.traverse((child) => {
      if (child.isMesh) {
        child.material.opacity = confidence;
      }
    });
  }

  _createFallbackBody(bodyId) {
    // Enhanced fallback skeleton with better proportions
    const group = new THREE.Group();
    group.name = `body-${bodyId}`;
    
    const joints = {};
    const limbs = {};
    const bones = [];
    
    // Joint positions (COCO 17-keypoint format)
    const jointPositions = {
      nose: [0, 1.7, 0],
      left_eye: [-0.03, 1.74, -0.02],
      right_eye: [0.03, 1.74, -0.02],
      left_ear: [-0.07, 1.72, 0],
      right_ear: [0.07, 1.72, 0],
      left_shoulder: [-0.22, 1.48, 0],
      right_shoulder: [0.22, 1.48, 0],
      left_elbow: [-0.45, 1.18, 0],
      right_elbow: [0.45, 1.18, 0],
      left_wrist: [-0.55, 0.92, 0],
      right_wrist: [0.55, 0.92, 0],
      left_hip: [-0.12, 0.98, 0],
      right_hip: [0.12, 0.98, 0],
      left_knee: [-0.12, 0.52, 0],
      right_knee: [0.12, 0.52, 0],
      left_ankle: [-0.12, 0.04, 0],
      right_ankle: [0.12, 0.04, 0]
    };

    // Create joint spheres with enhanced materials
    const jointGeom = new THREE.SphereGeometry(0.04, 16, 16);
    const headGeom = new THREE.SphereGeometry(0.12, 20, 20);
    
    for (const [name, pos] of Object.entries(jointPositions)) {
      const geom = name === 'nose' ? headGeom : jointGeom;
      const mat = name === 'nose' 
        ? this._materialCache.accent.clone()
        : this._materialCache.primary.clone();
      
      const mesh = new THREE.Mesh(geom, mat);
      mesh.position.set(...pos);
      mesh.castShadow = true;
      group.add(mesh);
      joints[name] = mesh;
    }

    // Create enhanced limbs with tapered cylinders
    const limbDefs = [
      { from: 'left_shoulder', to: 'left_elbow', radius: 0.035 },
      { from: 'right_shoulder', to: 'right_elbow', radius: 0.035 },
      { from: 'left_elbow', to: 'left_wrist', radius: 0.025 },
      { from: 'right_elbow', to: 'right_wrist', radius: 0.025 },
      { from: 'left_hip', to: 'left_knee', radius: 0.045 },
      { from: 'right_hip', to: 'right_knee', radius: 0.045 },
      { from: 'left_knee', to: 'left_ankle', radius: 0.035 },
      { from: 'right_knee', to: 'right_ankle', radius: 0.035 },
      { from: 'left_shoulder', to: 'right_shoulder', radius: 0.06 },
      { from: 'left_hip', to: 'right_hip', radius: 0.055 }
    ];

    for (const def of limbDefs) {
      const limb = this._createLimbMesh(
        jointPositions[def.from],
        jointPositions[def.to],
        def.radius,
        this._materialCache.secondary
      );
      group.add(limb);
      limbs[`${def.from}-${def.to}`] = limb;
    }

    // Add skeleton outline
    this._addSkeletonLines(group, jointPositions);

    return {
      id: bodyId,
      group: group,
      type: 'fallback',
      joints: joints,
      limbs: limbs,
      confidence: 0,
      update: (keypoints, confidence) => {
        this._updateFallbackBody(joints, limbs, keypoints, confidence);
      },
      setPosition: (x, y, z) => {
        group.position.set(x, y, z);
      },
      setVisible: (visible) => {
        group.visible = visible;
      },
      dispose: () => {
        group.traverse((child) => {
          if (child.isMesh) {
            child.geometry?.dispose();
            child.material?.dispose();
          }
        });
      }
    };
  }

  _createLimbMesh(from, to, radius, material) {
    const dir = new THREE.Vector3(
      to[0] - from[0],
      to[1] - from[1],
      to[2] - from[2]
    );
    const length = dir.length();
    
    const geom = new THREE.CylinderGeometry(radius * 0.8, radius, length, 12, 1);
    const mesh = new THREE.Mesh(geom, material.clone());
    
    // Position and orient
    const mid = new THREE.Vector3(
      (from[0] + to[0]) / 2,
      (from[1] + to[1]) / 2,
      (from[2] + to[2]) / 2
    );
    mesh.position.copy(mid);
    
    // Rotate to align with direction
    dir.normalize();
    const up = new THREE.Vector3(0, 1, 0);
    const quat = new THREE.Quaternion().setFromUnitVectors(up, dir);
    mesh.quaternion.copy(quat);
    
    mesh.castShadow = true;
    return mesh;
  }

  _addSkeletonLines(group, jointPositions) {
    const connections = [
      ['nose', 'left_eye'], ['nose', 'right_eye'],
      ['left_eye', 'left_ear'], ['right_eye', 'right_ear'],
      ['left_shoulder', 'right_shoulder'],
      ['left_shoulder', 'left_elbow'], ['left_elbow', 'left_wrist'],
      ['right_shoulder', 'right_elbow'], ['right_elbow', 'right_wrist'],
      ['left_shoulder', 'left_hip'], ['right_shoulder', 'right_hip'],
      ['left_hip', 'right_hip'],
      ['left_hip', 'left_knee'], ['left_knee', 'left_ankle'],
      ['right_hip', 'right_knee'], ['right_knee', 'right_ankle']
    ];

    for (const [a, b] of connections) {
      const positions = new Float32Array([
        ...jointPositions[a],
        ...jointPositions[b]
      ]);
      
      const geom = new THREE.BufferGeometry();
      geom.setAttribute('position', new THREE.BufferAttribute(positions, 3));
      
      const line = new THREE.Line(geom, this._materialCache.wireframe);
      group.add(line);
    }
  }

  _updateFallbackBody(joints, limbs, keypoints, confidence) {
    if (!keypoints || keypoints.length < 17) return;

    const updateJoint = (name, idx) => {
      if (joints[name] && keypoints[idx]) {
        const kp = keypoints[idx];
        if (kp.confidence > 0.1) {
          joints[name].position.set(kp[0], kp[1], kp[2]);
          joints[name].visible = true;
          
          // Adjust material based on confidence
          joints[name].material.emissiveIntensity = 0.3 + kp.confidence * 0.5;
        } else {
          joints[name].visible = false;
        }
      }
    };

    // Update all joints
    for (let i = 0; i < 17; i++) {
      updateJoint(Object.keys(joints)[i], i);
    }
  }

  // Main update method - call each frame
  update(personsData, delta) {
    if (!personsData || !Array.isArray(personsData)) return;

    const activeIds = new Set();

    for (const person of personsData) {
      const id = person.id || 'default';
      activeIds.add(id);

      let body = this.bodies.get(id);
      if (!body) {
        body = this.createBody(id);
      }

      if (body) {
        const keypoints = person.keypoints;
        const confidence = person.confidence || 0;
        
        body.setPosition(person.x || 0, person.y || 0, person.z || 0);
        body.setVisible(confidence > 0.1);
        body.update(keypoints, confidence);
      }
    }

    // Remove inactive bodies
    for (const [id, body] of this.bodies) {
      if (!activeIds.has(id)) {
        this.group.remove(body.group);
        body.dispose();
        this.bodies.delete(id);
      }
    }
  }

  getActiveCount() {
    return this.bodies.size;
  }

  dispose() {
    for (const body of this.bodies.values()) {
      this.group.remove(body.group);
      body.dispose();
    }
    this.bodies.clear();
    
    for (const model of this.loadedModels.values()) {
      model.traverse((child) => {
        if (child.isMesh) {
          child.geometry?.dispose();
        }
      });
    }
    this.loadedModels.clear();
  }
}

// Helper to load model from URL
export async function loadBodyModel(url) {
  const loader = new GLTFLoader();
  return new Promise((resolve, reject) => {
    loader.load(url, resolve, undefined, reject);
  });
}