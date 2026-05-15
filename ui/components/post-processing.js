// Post-Processing Effects - WiFi DensePose
// Bloom, glow effects for enhanced visuals

export class PostProcessing {
  constructor(renderer, scene, camera) {
    this.renderer = renderer;
    this.scene = scene;
    this.camera = camera;
    this.composer = null;
    this.bloomPass = null;
    
    this._init();
  }

  _init() {
    const width = this.renderer.domElement.width;
    const height = this.renderer.domElement.height;
    
    // Create composer
    this.composer = new THREE.EffectComposer(this.renderer);
    
    // Render pass
    const renderPass = new THREE.RenderPass(this.scene, this.camera);
    this.composer.addPass(renderPass);
    
    // Bloom pass - creates the glow effect
    this.bloomPass = new THREE.UnrealBloomPass(
      new THREE.Vector2(width, height),
      0.8,   // strength
      0.4,   // radius
      0.85   // threshold
    );
    this.composer.addPass(this.bloomPass);
  }

  // Adjust bloom intensity
  setBloomStrength(strength) {
    if (this.bloomPass) {
      this.bloomPass.strength = strength;
    }
  }

  setBloomRadius(radius) {
    if (this.bloomPass) {
      this.bloomPass.radius = radius;
    }
  }

  setBloomThreshold(threshold) {
    if (this.bloomPass) {
      this.bloomPass.threshold = threshold;
    }
  }

  // Render with post-processing
  render() {
    if (this.composer) {
      this.composer.render();
    }
  }

  // Handle resize
  setSize(width, height) {
    if (this.composer) {
      this.composer.setSize(width, height);
    }
  }

  dispose() {
    if (this.composer) {
      this.composer.dispose();
    }
  }
}

// Check if Three.js globals are available
function checkThreeExports() {
  return typeof THREE !== 'undefined' && 
         THREE.EffectComposer && 
         THREE.RenderPass && 
         THREE.UnrealBloomPass;
}

export function isPostProcessingSupported() {
  // In browser context, check on next tick
  if (typeof window !== 'undefined') {
    setTimeout(() => {
      console.log('[PostProcessing] Supported:', checkThreeExports());
    }, 100);
  }
  return typeof THREE !== 'undefined';
}