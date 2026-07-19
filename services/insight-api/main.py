import hashlib
import hmac as hmac_lib
import os
import base64
from datetime import datetime
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from dotenv import load_dotenv

load_dotenv()
app = FastAPI(title="RuView Insight API", version="0.1.0")
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"])

INGEST_SIGNING_KEY = os.getenv("INGEST_SIGNING_KEY", "")

class IngestRequest(BaseModel):
    session_id: str
    payload: str      # base64-encoded AES-256 encrypted JSON
    signature: str    # HMAC-SHA256 hex

def verify_signature(payload_b64: str, signature: str) -> bool:
    if not INGEST_SIGNING_KEY:
        return True  # dev mode
    key = INGEST_SIGNING_KEY.encode()
    expected = hmac_lib.new(key, payload_b64.encode(), hashlib.sha256).hexdigest()
    return hmac_lib.compare_digest(expected, signature)

@app.get("/health")
async def health():
    return {"status": "ok", "service": "insight-api"}

@app.post("/ingest")
async def ingest(req: IngestRequest):
    if not verify_signature(req.payload, req.signature):
        raise HTTPException(status_code=401, detail="Invalid signature")
    print(f"[ingest] session_id={req.session_id} at={datetime.utcnow().isoformat()}")
    return {"session_id": req.session_id, "status": "received"}

@app.get("/insights/{session_id}")
async def get_insight(session_id: str):
    return {
        "session_id": session_id,
        "status": "pending",
        "insight_text": None,
        "message": "RAG pipeline pending (feature/ai-insights-rag)"
    }
