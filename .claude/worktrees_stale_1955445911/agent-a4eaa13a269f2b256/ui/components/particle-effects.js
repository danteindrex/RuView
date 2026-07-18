// Particle Effects System - WiFi DensePose
// Animated particles for WiFi waves, motion trails, and ambient effects

export class ParticleSystem {
  constructor(scene, options = {}) {
    this.scene = scene;
    this.group = new THREE.Group();
    this.group.name = 'particle-effects';
    this.scene.add(this.group);

    this.config = {
      count: options.count || 1000,
      spread: options.spread || 5,
      color: options.color || 0x00ffff,
      size: options.size || 0.05,
      speed: options.speed || 1,
      lifeTime: options.lifeTime || 3
    };

    this.particles = [];
    this.activeParticles = new Set();
    
    // Create geometry and material once
    this._init();
  }

  _init() {
    const { count, color, size } = this.config;
    
    // Create buffer geometry
    this.geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(count * 3);
    const colors = new Float32Array(count * 3);
    const sizes = new Float32Array(count);
    const alphas = new Float32Array(count);

    for (let i = 0; i < count; i++) {
      positions[i * 3] = 0;
      positions[i * 3 + 1] = -1000; // Start off-screen
      positions[i * 3 + 2] = 0;
      
      const c = new THREE.Color(color);
      colors[i * 3] = c.r;
      colors[i * 3 + 1] = c.g;
      colors[i * 3 + 2] = c.b;
      
      sizes[i] = size * (0.5 + Math.random() * 0.5);
      alphas[i] = 0;
    }

    this.geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    this.geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));
    this.geometry.setAttribute('size', new THREE.BufferAttribute(sizes, 1));
    this.geometry.setAttribute('alpha', new THREE.BufferAttribute(alphas, 1));

    // Custom shader material for better looking particles
    const material = new THREE.ShaderMaterial({
      uniforms: {
        uTime: { value: 0 },
        uPixelRatio: { value: Math.min(window.devicePixelRatio, 2) }
      },
      vertexShader: `
        attribute float size;
        attribute float alpha;
        attribute vec3 color;
        varying vec3 vColor;
        varying float vAlpha;
        
        void main() {
          vColor = color;
          vAlpha = alpha;
          vec4 mvPosition = modelViewMatrix * vec4(position, 1.0);
          gl_PointSize = size * (300.0 / -mvPosition.z);
          gl_Position = projectionMatrix * mvPosition;
        }
      `,
      fragmentShader: `
        varying vec3 vColor;
        varying float vAlpha;
        
        void main() {
          float dist = length(gl_PointCoord - vec2(0.5));
          if (dist > 0.5) discard;
          
          float alpha = vAlpha * (1.0 - dist * 2.0);
          gl_FragColor = vec4(vColor, alpha);
        }
      `,
      transparent: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false
    });

    this.particleMesh = new THREE.Points(this.geometry, material);
    this.group.add(this.particleMesh);

    // Particle data tracking
    this.particleData = [];
    for (let i = 0; i < count; i++) {
      this.particleData.push({
        position: new THREE.Vector3(),
        velocity: new THREE.Vector3(),
        life: 0,
        maxLife: this.config.lifeTime,
        active: false
      });
    }
  }

  // Spawn particles at a position
  spawn(position, count = 10, options = {}) {
    let spawned = 0;
    
    for (let i = 0; i < this.particleData.length && spawned < count; i++) {
      const p = this.particleData[i];
      if (!p.active) {
        p.active = true;
        p.life = 0;
        p.maxLife = options.lifeTime || this.config.lifeTime;
        
        // Position with spread
        p.position.set(
          position.x + (Math.random() - 0.5) * this.config.spread * 0.5,
          position.y + (Math.random() - 0.5) * this.config.spread * 0.5,
          position.z + (Math.random() - 0.5) * this.config.spread * 0.5
        );
        
        // Velocity - outward expansion
        const speed = (options.speed || this.config.speed);
        p.velocity.set(
          (Math.random() - 0.5) * speed,
          (Math.random() - 0.5) * speed,
          (Math.random() - 0.5) * speed
        );
        
        // Custom color if provided
        if (options.color) {
          const idx = i * 3;
          const c = new THREE.Color(options.color);
          this.geometry.attributes.color.array[idx] = c.r;
          this.geometry.attributes.color.array[idx + 1] = c.g;
          this.geometry.attributes.color.array[idx + 2] = c.b;
        }
        
        spawned++;
        this.activeParticles.add(i);
      }
    }
  }

  // Spawn WiFi wave pattern (concentric rings)
  spawnWiFiWave(center, options = {}) {
    const rings = options.rings || 3;
    const particlesPerRing = options.particlesPerRing || 20;
    
    for (let r = 0; r < rings; r++) {
      const radius = 0.5 + r * 0.3;
      const delay = r * 0.1;
      
      for (let i = 0; i < particlesPerRing; i++) {
        const angle = (i / particlesPerRing) * Math.PI * 2;
        const x = center.x + Math.cos(angle) * radius;
        const y = center.y + Math.sin(angle * 2) * 0.2; // Wave
        const z = center.z + Math.sin(angle) * radius;
        
        this.spawnAt(x, y, z, 1, { ...options, lifeTime: 1 + r * 0.3 });
      }
    }
  }

  spawnAt(x, y, z, count = 1, options = {}) {
    const position = new THREE.Vector3(x, y, z);
    this.spawn(position, count, options);
  }

  // Update all particles
  update(delta, elapsed) {
    const positions = this.geometry.attributes.position.array;
    const alphas = this.geometry.attributes.alpha.array;
    const sizes = this.geometry.attributes.size.array;

    for (const i of this.activeParticles) {
      const p = this.particleData[i];
      p.life += delta;

      if (p.life >= p.maxLife) {
        p.active = false;
        positions[i * 3 + 1] = -1000; // Move off-screen
        alphas[i] = 0;
        this.activeParticles.delete(i);
        continue;
      }

      // Update position
      p.position.add(p.velocity.clone().multiplyScalar(delta));
      
      // Apply some drag
      p.velocity.multiplyScalar(0.98);
      
      // Update buffers
      const idx = i * 3;
      positions[idx] = p.position.x;
      positions[idx + 1] = p.position.y;
      positions[idx + 2] = p.position.z;
      
      // Fade in/out
      const lifeRatio = p.life / p.maxLife;
      alphas[i] = Math.sin(lifeRatio * Math.PI); // Fade in and out
      sizes[i] = this.config.size * (1 - lifeRatio * 0.5); // Shrink over time
    }

    this.geometry.attributes.position.needsUpdate = true;
    this.geometry.attributes.alpha.needsUpdate = true;
    this.geometry.attributes.size.needsUpdate = true;
    this.geometry.attributes.color.needsUpdate = true;
  }

  // Burst effect
  burst(position, count = 50, options = {}) {
    this.spawn(position, count, { 
      ...options, 
      speed: (options.speed || this.config.speed) * 2,
      lifeTime: options.lifeTime || 1
    });
  }

  dispose() {
    this.geometry.dispose();
    this.particleMesh.material.dispose();
    this.group.remove(this.particleMesh);
  }
}

// Trail effect for moving objects
export class TrailEffect {
  constructor(scene, options = {}) {
    this.scene = scene;
    this.trails = new Map();
    
    this.config = {
      maxLength: options.maxLength || 50,
      color: options.color || 0x00ffff,
      width: options.width || 0.02
    };
  }

  addTrail(id, points, color) {
    if (points.length < 2) return;

    // Create or update geometry
    let trail = this.trails.get(id);
    
    if (!trail) {
      const geometry = new THREE.BufferGeometry();
      const material = new THREE.LineBasicMaterial({
        color: color || this.config.color,
        transparent: true,
        opacity: 0.6,
        blending: THREE.AdditiveBlending
      });
      
      trail = new THREE.Line(geometry, material);
      this.scene.add(trail);
      
      this.trails.set(id, { mesh: trail, points: [] });
    }

    // Update points
    const trailData = this.trails.get(id);
    trailData.points.push(...points);
    
    // Limit length
    if (trailData.points.length > this.config.maxLength * 3) {
      trailData.points.splice(0, 3);
    }

    // Update geometry
    trail.mesh.geometry.setAttribute(
      'position',
      new THREE.Float32BufferAttribute(trailData.points, 3)
    );
    trail.mesh.geometry.attributes.position.needsUpdate = true;
  }

  removeTrail(id) {
    const trail = this.trails.get(id);
    if (trail) {
      this.scene.remove(trail.mesh);
      trail.mesh.geometry.dispose();
      trail.mesh.material.dispose();
      this.trails.delete(id);
    }
  }

  clear() {
    for (const id of this.trails.keys()) {
      this.removeTrail(id);
    }
  }
}

// Ambient floating particles
export class AmbientParticles {
  constructor(scene, options = {}) {
    this.scene = scene;
    this.group = new THREE.Group();
    this.scene.add(this.group);

    const count = options.count || 500;
    const spread = options.spread || 10;
    const height = options.height || 5;

    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(count * 3);
    const sizes = new Float32Array(count);
    const phases = new Float32Array(count);

    for (let i = 0; i < count; i++) {
      positions[i * 3] = (Math.random() - 0.5) * spread;
      positions[i * 3 + 1] = Math.random() * height;
      positions[i * 3 + 2] = (Math.random() - 0.5) * spread;
      
      sizes[i] = Math.random() * 0.03 + 0.01;
      phases[i] = Math.random() * Math.PI * 2;
    }

    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute('size', new THREE.BufferAttribute(sizes, 1));
    geometry.setAttribute('phase', new THREE.BufferAttribute(phases, 1));

    const material = new THREE.ShaderMaterial({
      uniforms: {
        uTime: { value: 0 },
        uColor: { value: new THREE.Color(options.color || 0x00aaff) }
      },
      vertexShader: `
        attribute float size;
        attribute float phase;
        varying float vAlpha;
        uniform float uTime;
        
        void main() {
          vec3 pos = position;
          pos.y += sin(uTime * 0.5 + phase) * 0.3;
          pos.x += sin(uTime * 0.3 + phase * 2.0) * 0.1;
          
          vAlpha = 0.3 + sin(uTime + phase) * 0.2;
          
          vec4 mvPosition = modelViewMatrix * vec4(pos, 1.0);
          gl_PointSize = size * (200.0 / -mvPosition.z);
          gl_Position = projectionMatrix * mvPosition;
        }
      `,
      fragmentShader: `
        uniform vec3 uColor;
        varying float vAlpha;
        
        void main() {
          float dist = length(gl_PointCoord - vec2(0.5));
          if (dist > 0.5) discard;
          gl_FragColor = vec4(uColor, vAlpha * (1.0 - dist * 2.0));
        }
      `,
      transparent: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false
    });

    this.particles = new THREE.Points(geometry, material);
    this.group.add(this.particles);
    
    this.material = material;
  }

  update(elapsed) {
    if (this.material) {
      this.material.uniforms.uTime.value = elapsed;
    }
  }

  setColor(color) {
    if (this.material) {
      this.material.uniforms.uColor.value.set(color);
    }
  }

  dispose() {
    this.particles.geometry.dispose();
    this.material.dispose();
    this.group.remove(this.particles);
  }
}