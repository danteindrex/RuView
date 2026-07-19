"""
Langfuse LlamaIndex instrumentation.
Import and call setup() in main.py lifespan startup.
"""
import os

def setup():
    try:
        from langfuse.llama_index import LlamaIndexInstrumentor
        pk = os.getenv("LANGFUSE_PUBLIC_KEY")
        sk = os.getenv("LANGFUSE_SECRET_KEY")
        if pk and sk:
            LlamaIndexInstrumentor(flush_interval=5).start()
            print(f"[langfuse] LlamaIndex tracing active → {os.getenv('LANGFUSE_HOST', 'cloud')}")
        else:
            print("[langfuse] keys not set — skipping instrumentation")
    except ImportError:
        print("[langfuse] SDK not installed (pip install langfuse) — skipping")
