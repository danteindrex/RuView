"""Insight API — LatentCSI vision service entry point."""
from fastapi import FastAPI

app = FastAPI(title="RuView Insight API", version="0.1.0")

from vision.generate import router as vision_router
app.include_router(vision_router)
