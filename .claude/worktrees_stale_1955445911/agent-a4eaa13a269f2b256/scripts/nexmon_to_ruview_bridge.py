#!/usr/bin/env python3
"""
Bridge Nexmon CSI UDP packets to RuView canonical CSI UDP frame format.

Use case:
- Raspberry Pi 4B running Nexmon CSI emits UDP packets on port 5500.
- RuView sensing-server expects canonical binary frames on UDP port 5005.
- This script translates one stream into the other.

Notes:
- Designed for bcm43455c0 integer CSI payloads (Pi 3B+/4B/5 path in Nexmon CSI).
- Not intended for bcm4358/bcm4366 floating-point CSI payloads.
"""

from __future__ import annotations

import argparse
import socket
import struct
import sys
import time
from dataclasses import dataclass
from typing import List, Tuple


NEXMON_MAGIC = 0x1111
ESP32_MAGIC = 0xC5110001


@dataclass
class NexmonPacket:
    src_mac: bytes
    seq: int
    core_ss: int
    chanspec: int
    chip: int
    iq: List[Tuple[int, int]]


def clamp_i8(value: int) -> int:
    if value > 127:
        return 127
    if value < -127:
        return -127
    return value


def signed_byte(v: int) -> int:
    """Convert signed int [-128..127] to one byte 0..255."""
    return v & 0xFF


def parse_nexmon_payload(payload: bytes) -> NexmonPacket:
    """
    Parse Nexmon CSI packet payload.

    Expected header (new format after #256 in nexmon_csi):
      0..1   magic (0x1111, LE)
      2..7   source MAC (6 bytes)
      8..9   sequence number (u16 LE)
      10..11 core/spatial stream (u16 LE)
      12..13 chanspec (u16 LE)
      14..15 chip version (u16 LE)
      16..   CSI bytes (int16 real, int16 imag per subcarrier for bcm43455c0)
    """
    if len(payload) < 20:
        raise ValueError("payload too short")

    magic = struct.unpack_from("<H", payload, 0)[0]
    if magic != NEXMON_MAGIC:
        raise ValueError(f"unexpected magic 0x{magic:04x}")

    src_mac = payload[2:8]
    seq = struct.unpack_from("<H", payload, 8)[0]
    core_ss = struct.unpack_from("<H", payload, 10)[0]
    chanspec = struct.unpack_from("<H", payload, 12)[0]
    chip = struct.unpack_from("<H", payload, 14)[0]

    csi = payload[16:]
    n_sc = len(csi) // 4
    if n_sc <= 0:
        raise ValueError("no CSI body")

    iq: List[Tuple[int, int]] = []
    for i in range(n_sc):
        off = i * 4
        real = struct.unpack_from("<h", csi, off)[0]
        imag = struct.unpack_from("<h", csi, off + 2)[0]
        iq.append((real, imag))

    return NexmonPacket(
        src_mac=src_mac,
        seq=seq,
        core_ss=core_ss,
        chanspec=chanspec,
        chip=chip,
        iq=iq,
    )


def core_from_core_ss(core_ss: int) -> int:
    # Nexmon docs: lowest 3 bits = core, next 3 bits = spatial stream.
    return core_ss & 0x7


def chanspec_to_freq_mhz(chanspec: int, fallback_freq: int) -> int:
    # Approximation: low byte is channel number in common Nexmon output.
    ch = chanspec & 0xFF
    if 1 <= ch <= 14:
        return 2407 + 5 * ch
    if 30 <= ch <= 196:
        return 5000 + 5 * ch
    return fallback_freq


def scale_iq_to_i8(iq: List[Tuple[int, int]]) -> List[Tuple[int, int]]:
    """Auto-scale int16 IQ values into signed int8 range."""
    peak = 1
    for i, q in iq:
        ai = abs(i)
        aq = abs(q)
        if ai > peak:
            peak = ai
        if aq > peak:
            peak = aq

    # Keep headroom to reduce clipping.
    scale = peak / 120.0 if peak > 120 else 1.0
    out: List[Tuple[int, int]] = []
    for i, q in iq:
        ii = clamp_i8(int(round(i / scale)))
        qq = clamp_i8(int(round(q / scale)))
        out.append((ii, qq))
    return out


def build_canonical_frame(
    iq_i8: List[Tuple[int, int]],
    node_id: int,
    seq: int,
    freq_mhz: int,
    rssi_dbm: int,
    noise_floor_dbm: int,
    max_subcarriers: int,
) -> bytes:
    # Canonical layout:
    # [0..3]  magic (u32)
    # [4]     node_id
    # [5]     n_antennas
    # [6..7]  n_subcarriers (u16)
    # [8..11] freq_mhz (u32)
    # [12..15] seq (u32)
    # [16]    rssi
    # [17]    noise_floor
    # [20..]  IQ i8 pairs
    if len(iq_i8) > max_subcarriers:
        iq_i8 = iq_i8[:max_subcarriers]

    n_sub = len(iq_i8)
    if n_sub <= 0:
        raise ValueError("empty IQ frame")

    frame = bytearray(20 + n_sub * 2)
    struct.pack_into("<I", frame, 0, ESP32_MAGIC)
    frame[4] = node_id & 0xFF
    frame[5] = 1  # n_antennas
    struct.pack_into("<H", frame, 6, n_sub & 0xFFFF)
    struct.pack_into("<I", frame, 8, freq_mhz & 0xFFFFFFFF)
    struct.pack_into("<I", frame, 12, seq & 0xFFFFFFFF)
    frame[16] = signed_byte(rssi_dbm)
    frame[17] = signed_byte(noise_floor_dbm)

    off = 20
    for i, q in iq_i8:
        frame[off] = signed_byte(i)
        frame[off + 1] = signed_byte(q)
        off += 2

    return bytes(frame)


def build_legacy_compat_frame(
    iq_i8: List[Tuple[int, int]],
    node_id: int,
    seq: int,
    freq_mhz: int,
    rssi_dbm: int,
    noise_floor_dbm: int,
    max_subcarriers: int,
) -> bytes:
    # Compatibility layout for older parsers:
    # [6] n_subcarriers u8, [8..9] freq u16, [10..13] seq, [14] rssi, [15] nf.
    if len(iq_i8) > max_subcarriers:
        iq_i8 = iq_i8[:max_subcarriers]

    n_sub = len(iq_i8)
    if n_sub <= 0:
        raise ValueError("empty IQ frame")

    frame = bytearray(20 + n_sub * 2)
    struct.pack_into("<I", frame, 0, ESP32_MAGIC)
    frame[4] = node_id & 0xFF
    frame[5] = 1
    frame[6] = n_sub & 0xFF
    frame[7] = 0
    struct.pack_into("<H", frame, 8, freq_mhz & 0xFFFF)
    struct.pack_into("<I", frame, 10, seq & 0xFFFFFFFF)
    frame[14] = signed_byte(rssi_dbm)
    frame[15] = signed_byte(noise_floor_dbm)

    off = 20
    for i, q in iq_i8:
        frame[off] = signed_byte(i)
        frame[off + 1] = signed_byte(q)
        off += 2

    return bytes(frame)


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Bridge Nexmon CSI UDP packets into RuView ESP32 UDP frames."
    )
    p.add_argument("--listen-host", default="0.0.0.0")
    p.add_argument("--listen-port", type=int, default=5500)
    p.add_argument("--out-host", default="127.0.0.1")
    p.add_argument("--out-port", type=int, default=5005)
    p.add_argument("--node-base", type=int, default=10, help="Base node id for mapped streams")
    p.add_argument("--default-rssi", type=int, default=-55, help="Fallback RSSI (dBm)")
    p.add_argument("--noise-floor", type=int, default=-92, help="Noise floor (dBm)")
    p.add_argument("--default-freq", type=int, default=2437, help="Fallback MHz if chanspec decode fails")
    p.add_argument(
        "--max-subcarriers",
        type=int,
        default=256,
        help="Limit subcarriers before bridge encoding",
    )
    p.add_argument(
        "--compat-layout",
        action="store_true",
        help="Use legacy shifted ESP32 frame layout for old server builds",
    )
    p.add_argument("--verbose-every", type=int, default=100)
    return p.parse_args()


def main() -> int:
    args = parse_args()

    rx = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    rx.bind((args.listen_host, args.listen_port))

    tx = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

    print(
        f"[bridge] listening {args.listen_host}:{args.listen_port} -> "
        f"{args.out_host}:{args.out_port}",
        flush=True,
    )
    print(
        "[bridge] note: bridge is compatibility-only. Native Pi agent is the preferred path.",
        flush=True,
    )
    if args.compat_layout:
        print("[bridge] using legacy compatibility frame layout", flush=True)
    else:
        print("[bridge] using canonical frame layout", flush=True)

    n_ok = 0
    n_err = 0
    t0 = time.time()

    while True:
        try:
            payload, _ = rx.recvfrom(65535)
            pkt = parse_nexmon_payload(payload)

            core = core_from_core_ss(pkt.core_ss)
            node_id = (args.node_base + core) & 0xFF
            freq_mhz = chanspec_to_freq_mhz(pkt.chanspec, args.default_freq)

            iq_i8 = scale_iq_to_i8(pkt.iq)
            if args.compat_layout:
                frame = build_legacy_compat_frame(
                    iq_i8=iq_i8,
                    node_id=node_id,
                    seq=pkt.seq,
                    freq_mhz=freq_mhz,
                    rssi_dbm=args.default_rssi,
                    noise_floor_dbm=args.noise_floor,
                    max_subcarriers=args.max_subcarriers,
                )
            else:
                frame = build_canonical_frame(
                    iq_i8=iq_i8,
                    node_id=node_id,
                    seq=pkt.seq,
                    freq_mhz=freq_mhz,
                    rssi_dbm=args.default_rssi,
                    noise_floor_dbm=args.noise_floor,
                    max_subcarriers=args.max_subcarriers,
                )
            tx.sendto(frame, (args.out_host, args.out_port))
            n_ok += 1

            if args.verbose_every > 0 and (n_ok % args.verbose_every == 0):
                dt = max(time.time() - t0, 1e-6)
                rate = n_ok / dt
                print(
                    f"[bridge] ok={n_ok} err={n_err} rate={rate:.1f}/s "
                    f"chip=0x{pkt.chip:04x} sc={len(pkt.iq)} core={core} chspec=0x{pkt.chanspec:04x}",
                    flush=True,
                )

        except KeyboardInterrupt:
            print("\n[bridge] stopped by user", flush=True)
            return 0
        except Exception as exc:
            n_err += 1
            if n_err <= 20 or n_err % 100 == 0:
                print(f"[bridge] drop #{n_err}: {exc}", file=sys.stderr, flush=True)


if __name__ == "__main__":
    raise SystemExit(main())
