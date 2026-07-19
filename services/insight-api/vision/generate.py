import time, base64, io, os, sys
import numpy as np
from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

router = APIRouter(prefix="/vision", tags=["vision"])

class VisionRequest(BaseModel):
    csi_amplitudes: list[float]
    prompt: str = ""
    strength: float = 0.6
    n_subcarriers: int = 342

class VisionResponse(BaseModel):
    image_base64: str
    generation_time_ms: int
    width: int = 512
    height: int = 512

@router.post("/generate", response_model=VisionResponse)
async def generate(req: VisionRequest):
    onnx_path = os.getenv("LATENTCSI_ONNX_PATH", "v1/models/latentcsi_encoder.onnx")
    if not os.path.exists(onnx_path):
        raise HTTPException(503, f"ONNX model not found at {onnx_path}. Run export_onnx.py first.")
    try:
        sys.path.insert(0, "v1/src/models/latentcsi")
        from inference import generate_image
        t0 = time.time()
        img = generate_image(np.array(req.csi_amplitudes, dtype=np.float32),
                             onnx_path, req.prompt, req.strength)
        ms = int((time.time() - t0) * 1000)
        buf = io.BytesIO(); img.save(buf, "PNG")
        return VisionResponse(image_base64=base64.b64encode(buf.getvalue()).decode(), generation_time_ms=ms)
    except Exception as e:
        raise HTTPException(500, str(e))
