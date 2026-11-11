"""Writes the tracker test fixture: a ProTracker module that holds one note
of a looped sine for 16 rows (1.92 s at speed 6, 125 bpm), then ends.

Usage: python3 make-mod.py <dir>
"""
import math, struct, sys

out = sys.argv[1]
# One period of a sine in 32 signed bytes. Period 428 plays it at
# 7093789.2 / (2 * 428) = 8287 Hz, so the tone is 259 Hz.
wave = bytes((int(100 * math.sin(2 * math.pi * i / 32)) & 0xFF) for i in range(32))
header = b"Popamp test".ljust(20, b"\0")
header += struct.pack(">22sHBBHH", b"sine", len(wave) // 2, 0, 64, 0, len(wave) // 2)
header += struct.pack(">22sHBBHH", b"", 0, 0, 0, 0, 1) * 30
header += bytes([1, 127]) + bytes(128) + b"M.K."

def cell(sample=0, period=0, effect=0, param=0):
    return bytes([(sample & 0xF0) | (period >> 8), period & 0xFF, ((sample & 0x0F) << 4) | effect, param])

rows = []
for row in range(64):
    first = cell(1, 428) if row == 0 else cell(effect=0xD) if row == 15 else cell()
    rows.append(first + cell() * 3)
open(out + "/sine.mod", "wb").write(header + b"".join(rows) + wave)
