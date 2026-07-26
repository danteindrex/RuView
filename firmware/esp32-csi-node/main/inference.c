// On-device CSI-embedding model inference (ESP32-S3).
// Numerically parity-matched to crates/wifi-densepose-edge-infer (Rust).
#include "inference.h"
#include "model_weights.h"
#include <math.h>
#include <stddef.h>

// Inference-mode BatchNorm1d: y = gamma*(x-mean)/sqrt(var+eps) + beta.
static inline float bn1d(float x, float g, float b, float m, float v) {
    return g * (x - m) / sqrtf(v + 1e-5f) + b;
}

float csi_model_presence(const float in[8], float *embedding_out) {
    // Layer 1: Linear(8->64) + BN + ReLU.
    float h[CSI_HID];
    for (int j = 0; j < CSI_HID; ++j) {
        float s = CSI_B1[j];
        for (int i = 0; i < CSI_IN; ++i) {
            s += CSI_W1[j * CSI_IN + i] * in[i];
        }
        s = bn1d(s, CSI_BN1_G[j], CSI_BN1_B[j], CSI_BN1_M[j], CSI_BN1_V[j]);
        h[j] = s > 0.0f ? s : 0.0f; // ReLU
    }

    // Layer 2: Linear(64->128) + BN.
    float e[CSI_EMB];
    for (int k = 0; k < CSI_EMB; ++k) {
        float s = CSI_B2[k];
        for (int j = 0; j < CSI_HID; ++j) {
            s += CSI_W2[k * CSI_HID + j] * h[j];
        }
        e[k] = bn1d(s, CSI_BN2_G[k], CSI_BN2_B[k], CSI_BN2_M[k], CSI_BN2_V[k]);
    }

    // Per-room LoRA: e' = e + scale * ((e . A) . B).
    float t[CSI_LORA_RANK];
    for (int r = 0; r < CSI_LORA_RANK; ++r) {
        float s = 0.0f;
        for (int i = 0; i < CSI_EMB; ++i) {
            s += e[i] * CSI_LORA_A[i * CSI_LORA_RANK + r];
        }
        t[r] = s;
    }
    for (int o = 0; o < CSI_EMB; ++o) {
        float s = 0.0f;
        for (int r = 0; r < CSI_LORA_RANK; ++r) {
            s += t[r] * CSI_LORA_B[r * CSI_EMB + o];
        }
        e[o] += CSI_LORA_SCALE * s;
        if (embedding_out) {
            embedding_out[o] = e[o];
        }
    }

    // Presence head: sigmoid(e' . w + b).
    float s = CSI_HEAD_B;
    for (int o = 0; o < CSI_EMB; ++o) {
        s += CSI_HEAD_W[o] * e[o];
    }
    return 1.0f / (1.0f + expf(-s));
}
