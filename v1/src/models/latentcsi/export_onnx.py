"""Export trained CsiEncoder checkpoint to ONNX (opset 17)."""
import sys, torch
from pathlib import Path

def export(ckpt_path: str, out_path: str = "v1/models/latentcsi_encoder.onnx"):
    from encoder import CsiEncoder
    ckpt = torch.load(ckpt_path, map_location="cpu")
    enc = CsiEncoder(ckpt["n_subcarriers"], ckpt["b"])
    enc.load_state_dict(ckpt["model_state_dict"])
    enc.eval()
    Path(out_path).parent.mkdir(parents=True, exist_ok=True)
    torch.onnx.export(enc, torch.randn(1, ckpt["n_subcarriers"]), out_path,
                      opset_version=17, input_names=["csi_amplitude"], output_names=["latent"],
                      dynamic_axes={"csi_amplitude":{0:"batch"},"latent":{0:"batch"}})
    print(f"Exported -> {out_path}")
    print(f"  Input: csi_amplitude [N, {ckpt['n_subcarriers']}]")
    print(f"  Output: latent [N, 4, 64, 64]")

if __name__ == "__main__":
    export(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else "v1/models/latentcsi_encoder.onnx")
