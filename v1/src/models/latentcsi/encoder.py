"""
LatentCSI CSI Encoder — arXiv:2506.10605
CSI amplitude [B, n_subcarriers] → SD v1.5 latent [B, 4, 64, 64]

Architecture from paper:
  FC → Reshape → 4× UpsampleBlock (last 3 with CrossAttention) → Conv2d
"""
import torch
import torch.nn as nn
import torch.nn.functional as F
from torch import Tensor


class ResBlock(nn.Module):
    def __init__(self, ch: int):
        super().__init__()
        self.net = nn.Sequential(
            nn.Conv2d(ch, ch, 3, padding=1), nn.BatchNorm2d(ch), nn.ReLU(),
            nn.Conv2d(ch, ch, 3, padding=1), nn.BatchNorm2d(ch),
        )
    def forward(self, x: Tensor) -> Tensor:
        return F.relu(self.net(x) + x)


class CrossAttentionBlock(nn.Module):
    def __init__(self, ch: int, heads: int = 4):
        super().__init__()
        self.norm = nn.GroupNorm(min(8, ch), ch)
        self.attn = nn.MultiheadAttention(ch, heads, batch_first=True)
    def forward(self, x: Tensor) -> Tensor:
        B, C, H, W = x.shape
        s = self.norm(x).flatten(2).transpose(1, 2)
        o, _ = self.attn(s, s, s)
        return x + o.transpose(1, 2).reshape(B, C, H, W)


class UpsampleBlock(nn.Module):
    def __init__(self, in_ch: int, out_ch: int, attn: bool = False):
        super().__init__()
        self.res1 = ResBlock(in_ch)
        self.res2 = ResBlock(in_ch)
        self.attn = CrossAttentionBlock(in_ch) if attn else nn.Identity()
        self.up = nn.ConvTranspose2d(in_ch, out_ch, 4, stride=2, padding=1)
    def forward(self, x: Tensor) -> Tensor:
        return self.up(self.attn(self.res2(self.res1(x))))


class CsiEncoder(nn.Module):
    def __init__(self, n_subcarriers: int, b: int = 256):
        super().__init__()
        self.b = b
        self.fc = nn.Linear(n_subcarriers, b * 4 * 4)
        self.up1 = UpsampleBlock(b,      b // 2,  attn=False)
        self.up2 = UpsampleBlock(b // 2, b // 4,  attn=True)
        self.up3 = UpsampleBlock(b // 4, b // 8,  attn=True)
        self.up4 = UpsampleBlock(b // 8, b // 16, attn=True)
        self.final = nn.Conv2d(b // 16, 4, 3, padding=1)

    def forward(self, x: Tensor) -> Tensor:
        B = x.shape[0]
        x = self.fc(x).reshape(B, self.b, 4, 4)
        return self.final(self.up4(self.up3(self.up2(self.up1(x)))))


if __name__ == "__main__":
    m = CsiEncoder(342, 256)
    params = sum(p.numel() for p in m.parameters())
    print(f"Params: {params:,}")
    out = m(torch.randn(2, 342))
    assert out.shape == (2, 4, 64, 64), f"bad shape: {out.shape}"
    print("CsiEncoder smoke test PASSED — output shape:", out.shape)
