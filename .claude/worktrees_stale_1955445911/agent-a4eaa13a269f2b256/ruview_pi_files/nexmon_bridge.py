#!/usr/bin/env python3
# /tmp/nexmon_bridge.py
import socket
import struct
import sys

# Read pcap stream from stdin (from tcpdump -w -)
# Forward UDP payloads to 127.0.0.1:5501
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

# Skip pcap global header (24 bytes)
sys.stdin.buffer.read(24)

while True:
    # Per-packet header: ts_sec, ts_usec, incl_len, orig_len (16 bytes)
    hdr = sys.stdin.buffer.read(16)
    if len(hdr) < 16:
        break
    ts_sec, ts_usec, incl_len, orig_len = struct.unpack('<IIII', hdr)
    pkt = sys.stdin.buffer.read(incl_len)
    
    # Skip Ethernet (14 bytes) + IP (20 bytes) + UDP (8 bytes) = 42 bytes
    payload = pkt[42:]
    if payload:
        sock.sendto(payload, ('127.0.0.1', 5501))