import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import * as SkeletonUtils from 'three/addons/utils/SkeletonUtils.js';

export const SKELETON_PAIRS = [];

const MAX_FIGURES = 4;
const _up = new THREE.Vector3(0, 1, 0);
const _vecTarget = new THREE.Vector3();

export class FigurePool {
  constructor(scene, settings) {
    this._scene = scene;
    this._settings = settings;
    this._figures = [];
    this._maxFigures = MAX_FIGURES;
    this._modelLoaded = false;
    
    this._loadModel();
  }
  
  get figures() { return this._figures; }
  
  async _loadModel() {
    const loader = new GLTFLoader();
    try {
      const gltf = await loader.loadAsync('/observatory/assets/models/Soldier.glb');
      const baseModel = gltf.scene;
      
      baseModel.traverse((child) => {
        if (child.isMesh) {
          child.castShadow = true;
          child.receiveShadow = true;
          // Apply a generic metallic material to look less like a stock asset and more like an observatory dummy
          child.material = new THREE.MeshStandardMaterial({
             color: 0x8899aa,
             roughness: 0.3,
             metalness: 0.8,
          });
        }
      });
      
      for (let i = 0; i < this._maxFigures; i++) {
        const group = new THREE.Group();
        const clone = SkeletonUtils.clone(baseModel);
        
        // Find bones
        const bones = {};
        clone.traverse(c => {
          if (c.isBone) bones[c.name] = c;
        });
        console.log("THREE.JS BONE NAMES:", Object.keys(bones).join(", "));
        
        
        // Adjust scale since Soldier.glb legs are longer than the demo keypoints
        clone.scale.set(0.95, 0.95, 0.95);
        group.add(clone);

        // Aura cylinder
        const wireColor = new THREE.Color(this._settings.wireColor);
        const auraGeo = new THREE.CylinderGeometry(0.4, 0.3, 1.7, 16, 1, true);
        const auraMat = new THREE.MeshBasicMaterial({
          color: wireColor, transparent: true, opacity: 0,
          side: THREE.DoubleSide, blending: THREE.AdditiveBlending, depthWrite: false,
        });
        const aura = new THREE.Mesh(auraGeo, auraMat);
        aura.position.y = 1;
        group.add(aura);

        // Per-figure light
        const personLight = new THREE.PointLight(wireColor, 0, 6);
        personLight.position.y = 1;
        group.add(personLight);

        this._scene.add(group);
        
        this._figures.push({
          group, clone, bones, aura, auraMat, personLight,
          visible: false,
          _initialized: false,
        });
      }
      this._modelLoaded = true;
    } catch (err) {
      console.error("Failed to load Soldier.glb", err);
    }
  }

  update(data, elapsed) {
    if (!this._modelLoaded) return;
    const persons = data?.persons || [];
    const vs = data?.vital_signs || {};
    const isPresent = data?.classification?.presence || false;
    const breathBpm = vs.breathing_rate_bpm || 0;
    const breathPulse = breathBpm > 0 ? Math.sin(elapsed * Math.PI * 2 * (breathBpm / 60)) * 0.012 : 0;

    for (let f = 0; f < this._figures.length; f++) {
      const fig = this._figures[f];
      const p = f < persons.length ? persons[f] : null;
      // Only draw a skeleton when the person carries REAL model keypoints
      // (keypoints17, set only when pose_source === "model_inference"). We never
      // synthesize a pose — presence-without-model shows a HUD notice instead.
      if (p && isPresent && Array.isArray(p.keypoints17) && p.keypoints17.length >= 17) {
        this.applyKeypoints(fig, p.keypoints17, breathPulse, p.position || [0, 0, 0], elapsed, p.pose);
        fig.visible = true;
        fig.group.visible = true;
      } else {
        if (fig.visible) {
          this.hide(fig);
        }
      }
    }
  }

  _pointBone(bone, startWorld, endWorld) {
    if (!bone) return;
    const dir = new THREE.Vector3().subVectors(endWorld, startWorld).normalize();
    if (dir.lengthSq() < 0.001) return;
    
    const parentQuat = new THREE.Quaternion();
    bone.parent.getWorldQuaternion(parentQuat);
    const localDir = dir.clone().applyQuaternion(parentQuat.invert());
    
    // Smooth interpolation to avoid snapping
    const targetQuat = new THREE.Quaternion().setFromUnitVectors(_up, localDir);
    bone.quaternion.slerp(targetQuat, 0.4); // Smooth transition
    bone.updateMatrixWorld(true);
  }

  applyKeypoints(fig, kps, breathPulse, pos, elapsed, pose) {
    if (!kps || kps.length < 17) return;
    
    const vec = (i) => new THREE.Vector3(...kps[i]);
    
    fig.group.position.set(0, 0, 0); 
    
    const hips = fig.bones['mixamorigHips'];
    if (hips) {
       const hipPos = vec(11).clone().lerp(vec(12), 0.5);
       
       // Convert world space hipPos to the local space of the armature (because clone is scaled)
       const localHipPos = hips.parent.worldToLocal(hipPos.clone());
       _vecTarget.copy(localHipPos);
       
       if (fig._initialized) {
          hips.position.lerp(_vecTarget, 0.4);
       } else {
          hips.position.copy(_vecTarget);
       }
       hips.updateMatrixWorld(true);
    }
    
    // Arms
    this._pointBone(fig.bones['mixamorigLeftArm'], vec(5), vec(7));
    this._pointBone(fig.bones['mixamorigLeftForeArm'], vec(7), vec(9));
    this._pointBone(fig.bones['mixamorigRightArm'], vec(6), vec(8));
    this._pointBone(fig.bones['mixamorigRightForeArm'], vec(8), vec(10));
    
    // Legs
    this._pointBone(fig.bones['mixamorigLeftUpLeg'], vec(11), vec(13));
    this._pointBone(fig.bones['mixamorigLeftLeg'], vec(13), vec(15));
    this._pointBone(fig.bones['mixamorigRightUpLeg'], vec(12), vec(14));
    this._pointBone(fig.bones['mixamorigRightLeg'], vec(14), vec(16));
    
    // Spine
    const neckPos = vec(5).clone().lerp(vec(6), 0.5);
    const hipCenter = vec(11).clone().lerp(vec(12), 0.5);
    this._pointBone(fig.bones['mixamorigSpine'], hipCenter, neckPos);
    
    // Head/Neck (Nose is 0)
    this._pointBone(fig.bones['mixamorigNeck'], neckPos, vec(0));
    
    // Aura and lights
    fig.aura.position.set(pos[0], pos[1] + 1, pos[2]);
    fig.auraMat.opacity = this._settings.aura + Math.abs(breathPulse) * 0.8;
    fig.personLight.position.set(pos[0], pos[1] + 1.2, pos[2]);
    fig.personLight.intensity = this._settings.glow * 0.4;
    
    fig._initialized = true;
  }
  
  hide(fig) {
    fig.group.visible = false;
    fig.visible = false;
    fig._initialized = false;
  }

  applyColors(wireColor, jointColor) {
    for (const fig of this._figures) {
      fig.auraMat.color.copy(wireColor);
      fig.personLight.color.copy(wireColor);
    }
  }
}
