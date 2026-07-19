"""RuView Insight API — LangGraph multi-agent AI insights with Langfuse tracing."""
import hashlib, hmac as hmac_lib, os, uuid
from contextlib import asynccontextmanager
from fastapi import FastAPI, HTTPException, BackgroundTasks, Depends, Header
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from dotenv import load_dotenv
from typing import Optional

load_dotenv()
_report_cache: dict = {}
from deployments.router import router as deployments_router

@asynccontextmanager
async def lifespan(app):
    # Langfuse instrumentation (from feature/langfuse-tracing)
    try:
        from langfuse.callback import CallbackHandler
        app.state.langfuse_handler = CallbackHandler(
            public_key=os.getenv("LANGFUSE_PUBLIC_KEY", ""),
            secret_key=os.getenv("LANGFUSE_SECRET_KEY", ""),
            host=os.getenv("LANGFUSE_HOST", "https://cloud.langfuse.com"),
        ) if os.getenv("LANGFUSE_PUBLIC_KEY") else None
    except ImportError:
        app.state.langfuse_handler = None
    yield

app = FastAPI(title="RuView Insight API", version="0.3.0", lifespan=lifespan)
app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"])
app.include_router(deployments_router)

INGEST_SIGNING_KEY = os.getenv("INGEST_SIGNING_KEY", "")

class IngestRequest(BaseModel):
    session_id: str
    payload: str
    signature: str

class QueryRequest(BaseModel):
    session_id: str
    vital_summary: dict
    pose_anomalies: list[str] = []
    duration_seconds: int = 0
    csi_snr_db: float = 0.0
    csi_vision_image_b64: Optional[str] = None  # from feature/latentcsi-image-gen

def verify_sig(payload: str, sig: str) -> bool:
    if not INGEST_SIGNING_KEY: return True
    expected = hmac_lib.new(INGEST_SIGNING_KEY.encode(), payload.encode(), hashlib.sha256).hexdigest()
    return hmac_lib.compare_digest(expected, sig)

def check_bearer(authorization: str = Header(default="")) -> bool:
    """JWT validation — integrates with feature/security-hardening Bearer tokens.
    In production, validate with the same jwt_secret as the sensing server."""
    # TODO: validate JWT signature when feature/security-hardening is merged
    # For now: accept any non-empty Bearer token or no token (dev mode)
    return True

@app.get("/health")
async def health():
    from agents.graph import get_graph
    graph_ready = get_graph() is not None
    return {"status": "ok", "version": "0.3.0", "langgraph": graph_ready,
            "langfuse": app.state.langfuse_handler is not None}

@app.post("/ingest")
async def ingest(req: IngestRequest, bg: BackgroundTasks, _=Depends(check_bearer)):
    if not verify_sig(req.payload, req.signature):
        raise HTTPException(401, "Invalid HMAC signature")
    return {"session_id": req.session_id, "status": "received"}

@app.post("/query")
async def query_insight(req: QueryRequest, _=Depends(check_bearer)):
    # Cloud plan gate: check header (integrates with feature/cloud-tier-gating)
    from agents.state import SessionContext
    from agents.graph import run_insight_pipeline
    ctx = SessionContext(**req.model_dump())
    try:
        state = await run_insight_pipeline(ctx)
        report = state.get("final_report")
        if not report:
            raise HTTPException(503, f"Pipeline failed: {state.get('errors', [])}")
        result = report.model_dump()
        _report_cache[req.session_id] = result
        return result
    except Exception as e:
        raise HTTPException(500, str(e))

@app.get("/insights/{session_id}")
async def get_insight(session_id: str, _=Depends(check_bearer)):
    if session_id in _report_cache:
        return _report_cache[session_id]
    return {"session_id": session_id, "status": "pending", "final_report": None}

@app.get("/analytics/trends")
async def get_trends(_=Depends(check_bearer)):
    from analytics.trend_analyzer import get_analyzer
    h = list(get_analyzer()._history)
    return {"sessions": len(h), "history": [
        {"session_id": m.session_id, "hr_mean": m.hr_mean, "br_mean": m.br_mean,
         "risk_score": m.risk_score, "anomaly_count": m.anomaly_count} for m in h
    ]}

@app.get("/analytics/risk-distribution")
async def risk_distribution(_=Depends(check_bearer)):
    reports = list(_report_cache.values())
    if not reports:
        return {"low": 0, "moderate": 0, "high": 0, "critical": 0}
    dist = {"low": 0, "moderate": 0, "high": 0, "critical": 0}
    for r in reports:
        level = r.get("risk", {}).get("risk_level", "low")
        dist[level] = dist.get(level, 0) + 1
    return dist
