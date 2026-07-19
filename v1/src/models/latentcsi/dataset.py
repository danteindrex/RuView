"""MM-Fi and custom paired CSI+image dataset loader."""
import numpy as np
from pathlib import Path
from torch.utils.data import Dataset
from torchvision import transforms
from PIL import Image
try:
    import scipy.io as sio
    HAS_SCIPY = True
except ImportError:
    HAS_SCIPY = False


class MmFiCsiDataset(Dataset):
    """
    Expects:
      data_root/wifi/*.mat   — complex CSI, amplitude = sqrt(re²+im²)
      data_root/rgb/*.jpg    — synchronized 512×512 RGB frames
    Split: 80/10/10 train/val/test by stable shuffle (seed=42).
    """
    def __init__(self, data_root: str, split: str = "train", seed: int = 42):
        if not HAS_SCIPY:
            raise ImportError("pip install scipy")
        self.tf = transforms.Compose([
            transforms.Resize((512, 512)),
            transforms.ToTensor(),
            transforms.Normalize([0.5]*3, [0.5]*3),
        ])
        wifi_dir = Path(data_root) / "wifi"
        rgb_dir  = Path(data_root) / "rgb"
        mats = sorted(wifi_dir.glob("*.mat")) if wifi_dir.exists() else []
        jpgs = sorted(rgb_dir.glob("*.jpg"))  if rgb_dir.exists()  else []
        pairs = list(zip(mats, jpgs))
        rng = np.random.default_rng(seed)
        idx = rng.permutation(len(pairs))
        n = len(pairs)
        slices = {"train": idx[:int(0.8*n)], "val": idx[int(0.8*n):int(0.9*n)], "test": idx[int(0.9*n):]}
        self.pairs = [pairs[i] for i in slices[split]]

    def __len__(self): return len(self.pairs)

    def __getitem__(self, i):
        mat_p, jpg_p = self.pairs[i]
        mat = sio.loadmat(str(mat_p))
        key = next((k for k in ["csi_data","csi","data"] if k in mat), list(mat.keys())[-1])
        csi = mat[key]
        amp = np.sqrt(csi.real**2 + csi.imag**2).flatten().astype(np.float32)
        img = Image.open(jpg_p).convert("RGB")
        return amp, self.tf(img)
