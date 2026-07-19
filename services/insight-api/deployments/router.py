"""Multi-location deployment registry and heartbeat API."""
import os
from datetime import datetime, timezone
from typing import Optional
from fastapi import APIRouter, HTTPException, Depends, Header
from pydantic import BaseModel

router = APIRouter(prefix="/deployments", tags=["deployments"])

# In-memory store (replace with Postgres in production)
_deployments: dict[str, dict] = {}

class DeploymentRegisterRequest(BaseModel):
    deployment_id: str
    deployment_name: str
    location_name: str
    latitude: Optional[float] = None
    longitude: Optional[float] = None
    tenant_id: Optional[str] = None
    node_count: Optional[int] = None
    active_risk_level: Optional[str] = None

class DeploymentStatus(BaseModel):
    deployment_id: str
    deployment_name: str
    location_name: str
    latitude: Optional[float] = None
    longitude: Optional[float] = None
    tenant_id: Optional[str] = None
    last_seen: Optional[str] = None
    node_count: Optional[int] = None
    active_risk_level: Optional[str] = None
    online: bool = False

def _is_online(last_seen_iso: Optional[str]) -> bool:
    if not last_seen_iso:
        return False
    try:
        last = datetime.fromisoformat(last_seen_iso.replace("Z", "+00:00"))
        delta = (datetime.now(timezone.utc) - last).total_seconds()
        return delta < 120  # online if heartbeat within 2 minutes
    except Exception:
        return False

@router.post("/register")
async def register_deployment(req: DeploymentRegisterRequest):
    """Register or update a deployment (called on every Tauri app startup)."""
    now = datetime.now(timezone.utc).isoformat()
    existing = _deployments.get(req.deployment_id, {})
    _deployments[req.deployment_id] = {
        **existing,
        "deployment_id": req.deployment_id,
        "deployment_name": req.deployment_name,
        "location_name": req.location_name,
        "latitude": req.latitude,
        "longitude": req.longitude,
        "tenant_id": req.tenant_id,
        "last_seen": now,
        "node_count": req.node_count or existing.get("node_count"),
        "active_risk_level": req.active_risk_level or existing.get("active_risk_level", "low"),
    }
    return {"status": "registered", "deployment_id": req.deployment_id, "last_seen": now}

@router.get("")
async def list_deployments(tenant_id: Optional[str] = None) -> list[DeploymentStatus]:
    """List all deployments, optionally filtered by tenant_id."""
    result = []
    for d in _deployments.values():
        if tenant_id and d.get("tenant_id") != tenant_id:
            continue
        result.append(DeploymentStatus(
            **{k: v for k, v in d.items() if k in DeploymentStatus.model_fields},
            online=_is_online(d.get("last_seen")),
        ))
    return sorted(result, key=lambda x: x.deployment_name)

@router.get("/aggregate")
async def get_aggregate(tenant_id: Optional[str] = None):
    """Aggregate stats across all deployments for a tenant."""
    deps = [d for d in _deployments.values()
            if not tenant_id or d.get("tenant_id") == tenant_id]
    total = len(deps)
    online = sum(1 for d in deps if _is_online(d.get("last_seen")))
    high_risk = sum(1 for d in deps if d.get("active_risk_level") in ("high", "critical"))
    risk_map = {"low": 10, "moderate": 40, "high": 70, "critical": 90}
    avg_risk = (
        sum(risk_map.get(d.get("active_risk_level", "low"), 10) for d in deps) / total
        if total else 0
    )
    return {
        "total": total,
        "online": online,
        "offline": total - online,
        "high_risk": high_risk,
        "avg_risk_score": round(avg_risk, 1),
    }

@router.patch("/{deployment_id}/risk")
async def update_risk(deployment_id: str, risk_level: str, node_count: Optional[int] = None):
    """Called by a deployment to update its current risk level (from AI insights pipeline)."""
    if deployment_id not in _deployments:
        raise HTTPException(404, "Deployment not found")
    now = datetime.now(timezone.utc).isoformat()
    _deployments[deployment_id]["active_risk_level"] = risk_level
    _deployments[deployment_id]["last_seen"] = now
    if node_count is not None:
        _deployments[deployment_id]["node_count"] = node_count
    return {"status": "updated", "last_seen": now}

@router.get("/{deployment_id}")
async def get_deployment(deployment_id: str) -> DeploymentStatus:
    if deployment_id not in _deployments:
        raise HTTPException(404, "Deployment not found")
    d = _deployments[deployment_id]
    return DeploymentStatus(**{k: v for k, v in d.items() if k in DeploymentStatus.model_fields},
                             online=_is_online(d.get("last_seen")))
