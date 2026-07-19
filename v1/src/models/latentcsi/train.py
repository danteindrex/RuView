"""Train CsiEncoder to map CSI amplitude → SD v1.5 VAE latent mean."""
import os, sys
import torch
import torch.nn.functional as F
from torch.utils.data import DataLoader
from pathlib import Path


def train(data_root, n_subcarriers=342, b=256, lr=5e-4, batch_size=16,
          max_epochs=100, patience=5, checkpoint_dir="v1/models/latentcsi_checkpoints"):
    from encoder import CsiEncoder
    from dataset import MmFiCsiDataset
    try:
        from diffusers import AutoencoderKL
    except ImportError:
        sys.exit("pip install diffusers transformers accelerate")

    device = "cuda" if torch.cuda.is_available() else "cpu"
    print(f"Device: {device}")

    vae = AutoencoderKL.from_pretrained("runwayml/stable-diffusion-v1-5", subfolder="vae").to(device)
    for p in vae.parameters(): p.requires_grad_(False)
    vae.eval()

    encoder = CsiEncoder(n_subcarriers, b).to(device)
    opt = torch.optim.Adam(encoder.parameters(), lr=lr)

    train_dl = DataLoader(MmFiCsiDataset(data_root, "train"), batch_size=batch_size, shuffle=True, num_workers=4)
    val_dl   = DataLoader(MmFiCsiDataset(data_root, "val"),   batch_size=batch_size, num_workers=2)

    Path(checkpoint_dir).mkdir(parents=True, exist_ok=True)
    best, no_improve = float("inf"), 0

    for ep in range(max_epochs):
        encoder.train()
        tr_loss = 0
        for csi, imgs in train_dl:
            csi, imgs = csi.to(device), imgs.to(device)
            with torch.no_grad():
                target = vae.encode(imgs).latent_dist.mean
            loss = F.mse_loss(encoder(csi), target)
            opt.zero_grad(); loss.backward(); opt.step()
            tr_loss += loss.item()
        tr_loss /= len(train_dl)

        encoder.eval(); val_loss = 0
        with torch.no_grad():
            for csi, imgs in val_dl:
                csi, imgs = csi.to(device), imgs.to(device)
                val_loss += F.mse_loss(encoder(csi), vae.encode(imgs).latent_dist.mean).item()
        val_loss /= len(val_dl)
        print(f"Epoch {ep+1}/{max_epochs}  train={tr_loss:.4f}  val={val_loss:.4f}")

        if val_loss < best:
            best, no_improve = val_loss, 0
            torch.save({"epoch":ep,"model_state_dict":encoder.state_dict(),
                        "val_loss":val_loss,"n_subcarriers":n_subcarriers,"b":b},
                       f"{checkpoint_dir}/best.pt")
            print(f"  -> checkpoint saved (val={val_loss:.4f})")
        else:
            no_improve += 1
            if no_improve >= patience:
                print(f"Early stop at epoch {ep+1}"); break


if __name__ == "__main__":
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("--data-root", required=True)
    ap.add_argument("--n-subcarriers", type=int, default=342)
    ap.add_argument("--b", type=int, default=256)
    args = ap.parse_args()
    train(args.data_root, args.n_subcarriers, args.b)
