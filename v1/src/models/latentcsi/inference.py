"""Run LatentCSI inference: CSI → SD v1.5 latent → 512×512 image."""
import numpy as np
from PIL import Image
import onnxruntime as ort
import torch


def generate_image(csi_amplitude: np.ndarray, onnx_path: str = "v1/models/latentcsi_encoder.onnx",
                   prompt: str = "", strength: float = 0.6, num_steps: int = 100) -> Image.Image:
    from diffusers import StableDiffusionImg2ImgPipeline, DDIMScheduler

    sess = ort.InferenceSession(onnx_path)
    inp = csi_amplitude.astype(np.float32).reshape(1, -1)
    latent = torch.from_numpy(sess.run(["latent"], {"csi_amplitude": inp})[0])

    pipe = StableDiffusionImg2ImgPipeline.from_pretrained(
        "runwayml/stable-diffusion-v1-5",
        torch_dtype=torch.float32,
        scheduler=DDIMScheduler.from_pretrained("runwayml/stable-diffusion-v1-5", subfolder="scheduler"),
    )
    with torch.no_grad():
        decoded = pipe.vae.decode(latent / pipe.vae.config.scaling_factor).sample

    img_np = ((decoded[0].permute(1,2,0).numpy() + 1) / 2 * 255).clip(0, 255).astype(np.uint8)
    base_img = Image.fromarray(img_np)

    if prompt:
        result = pipe(prompt=prompt, image=base_img, strength=strength,
                      num_inference_steps=num_steps).images[0]
        return result
    return base_img
