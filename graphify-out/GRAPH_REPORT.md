# RuView WiFi-DensePose Knowledge Graph Report

## Graph Statistics
- **Nodes**: 49 concepts
- **Edges**: 47 relationships
- **Hyperedges**: 7 communities
- **Communities**: 9 domains

## God Nodes (Most Connected Concepts)

| Concept | Connections | Role |
|---------|-------------|------|
| RuvSense Pipeline | 7 | Central orchestrator for multistatic sensing |
| WiFlow Architecture | 5 | Neural network backbone for pose estimation |
| MERIDIAN Architecture | 4 | Domain generalization across environments |
| Coherence Gate | 3 | Decision boundary for environmental drift |
| VitalAnomalyDetector | 3 | Clinical anomaly detection for vitals |

## Communities

### 1. Signal Processing (8 concepts)
SpotFi Phase Cleaning → Phase Unwrapping ← Hampel Filter
Fresnel Zone Model → Body Velocity Profile
CSI Spectrogram ↔ Conjugate Multiplication

Key insight: Phase cleaning (SpotFi + conjugate multiply) feeds into Fresnel zone modeling which enables BVP extraction.

### 2. Neural Architecture (6 concepts)
WiFlow Architecture → TCN + Axial Self-Attention + Asymmetric Conv Encoder
DensePose Head ← COCO-17 Keypoints

Key insight: WiFlow combines TCN (dilation 1,2,4,8) with axial attention (O(H²W + HW²) vs O(H²W²)) for efficient pose estimation.

### 3. RuvSense Multistatic Pipeline (7 concepts)
Pipeline: Multiband Fusion → Phase Aligner → Multistatic Fusion → Coherence Scoring → Coherence Gate → Pose Tracker

Key insight: 6-stage pipeline with Kalman pose tracking + AETHER re-ID embeddings for identity persistence.

### 4. RuVector Integration (4 concepts)
RuVector Attention ← BVP
RuVector Solver ← Fresnel geometry
RuVector MinCut ← Subcarrier selection
RuVector Temporal Tensor ← CompressedCsiBuffer

Key insight: 5-crate integration replaces hand-tuned thresholds with learned attention weights.

### 5. Vital Signs (5 concepts)
IIR Bandpass → Zero-Crossing (breathing) / Autocorrelation Peak (heart rate)
Phase Coherence Weighting enhances cardiac detection

### 6. Domain Adaptation (4 concepts)
MERIDIAN: DomainFactorizer + Gradient Reversal Layer + FiLM conditioning
Enables cross-environment deployment without retraining.

## Surprising Connections

1. **WiFlow ↔ AETHER**: The pose estimation network and contrastive embedding share conceptual architecture — both use attention mechanisms for spatial reasoning.

2. **RuvSense Pipeline ↔ CrossViewpointAttention**: The multistatic fusion module and cross-viewpoint attention both implement attention-weighted aggregation with geometric bias.

3. **Neumann Solver ↔ Fresnel Zone**: The iterative solver from ruvector-solver is used for geometry estimation — an unexpected bridge between numerical methods and RF sensing.

4. **Welford Statistics ↔ VitalAnomalyDetector**: Running statistics for drift detection also serve as the foundation for clinical anomaly detection (apnea, tachycardia).

## Suggested Questions

1. **How does phase coherence weighting improve heart rate extraction?**
   Trace: Phase Coherence Weighting → Autocorrelation Peak Detection → HeartRateExtractor

2. **What enables RuvSense to track multiple people without identity swaps?**
   Trace: AETHER Contrastive Embedding → Pose Tracker → Kalman Pose Tracking → COCO-17

3. **How does MERIDIAN achieve cross-environment generalization?**
   Trace: MERIDIAN → FiLM Conditioning → DomainFactorizer → Gradient Reversal Layer

4. **What connects signal processing to neural network training?**
   Trace: CSI Spectrogram → Body Velocity Profile → RuVector Attention → WiFlow Architecture

5. **How does the ESP32 firmware integrate with the Rust pipeline?**
   Trace: ESP32-S3 CSI → TDM Mesh Protocol → Processing Tiers → Adaptive Controller

## Architecture Overview

```
WiFi Signal → ESP32 CSI → Phase Sanitization (SpotFi + Hampel)
                         ↓
              Fresnel Zone + BVP Extraction
                         ↓
              RuVector Integration (5 crates)
                         ↓
        ┌────────────────┴────────────────┐
        ↓                                 ↓
   RuvSense Pipeline            WiFlow Neural Net
   (Multistatic + Pose          (TCN + Axial Attention
    Tracking)                     + Pose Decoder)
        ↓                                 ↓
   Cross-Viewpoint Fusion        DensePose + COCO-17
        ↓                                 ↓
   Kalman Tracker ← AETHER Re-ID → Identity Persistence
        ↓
   Vital Signs (Breathing + Heart Rate)
        ↓
   MERIDIAN Domain Adaptation (cross-environment)
```

## Key Architectural Decisions

| ADR | Decision | Impact |
|-----|----------|--------|
| ADR-014 | SOTA signal processing (SpotFi, Hampel, Fresnel) | Foundation for all CSI interpretation |
| ADR-016 | RuVector 5-crate integration | Replaces 200+ hand-tuned thresholds |
| ADR-024 | AETHER contrastive embedding | Zero identity swaps over 10 minutes |
| ADR-027 | MERIDIAN + FiLM conditioning | Deploy in any room without retraining |
| ADR-029/030 | RuvSense 7-tier exotic capabilities | Enables RF tomography, gesture, adversarial detection |
| ADR-072 | WiFlow architecture (TCN + axial attention) | 92.9% PCK@0.2 on MM-Fi dataset |

---
*Generated by graphify on 2026-05-13 from 3 parallel extraction agents (350K tokens)*