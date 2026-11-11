"""Writes the MIDI test fixtures: a one-preset sine SoundFont and a one-second A4.

Usage: python3 make-midi.py <dir>
"""
import math, struct, sys

out = sys.argv[1]

def chunk(tag, data):
    pad = b"\0" if len(data) % 2 else b""
    return tag + struct.pack("<I", len(data)) + data + pad

def lst(kind, *chunks):
    return chunk(b"LIST", kind + b"".join(chunks))

def name(s):
    return s.encode().ljust(20, b"\0")

# Names are even in length: SF2 strings are padded to it, and readers count on it.
# Ten periods of 440 Hz at 44 kHz, half scale, then the 46 zeros SF2 asks for.
samples = [int(16384 * math.sin(2 * math.pi * i / 100)) for i in range(1000)] + [0] * 46
smpl = struct.pack("<%dh" % len(samples), *samples)

phdr = struct.pack("<20sHHHIII", name("Sine"), 0, 0, 0, 0, 0, 0) + struct.pack("<20sHHHIII", name("EOP"), 0, 0, 1, 0, 0, 0)
pbag = struct.pack("<HH", 0, 0) + struct.pack("<HH", 1, 0)
pmod = b"\0" * 10
pgen = struct.pack("<HH", 41, 0) + struct.pack("<HH", 0, 0)  # instrument 0
inst = struct.pack("<20sH", name("Sine"), 0) + struct.pack("<20sH", name("EOI"), 1)
ibag = struct.pack("<HH", 0, 0) + struct.pack("<HH", 2, 0)
imod = b"\0" * 10
igen = struct.pack("<HH", 54, 1) + struct.pack("<HH", 53, 0) + struct.pack("<HH", 0, 0)  # loop, sample 0
shdr = struct.pack("<20sIIIIIBbHH", name("Sine"), 0, 1000, 100, 900, 44000, 69, 0, 0, 1) + struct.pack("<20sIIIIIBbHH", name("EOS"), 0, 0, 0, 0, 0, 0, 0, 0, 0)

body = b"sfbk" + lst(b"INFO", chunk(b"ifil", struct.pack("<HH", 2, 1)), chunk(b"isng", b"EMU8000\0"), chunk(b"INAM", b"Popamp sine\0")) \
    + lst(b"sdta", chunk(b"smpl", smpl)) \
    + lst(b"pdta", chunk(b"phdr", phdr), chunk(b"pbag", pbag), chunk(b"pmod", pmod), chunk(b"pgen", pgen),
          chunk(b"inst", inst), chunk(b"ibag", ibag), chunk(b"imod", imod), chunk(b"igen", igen), chunk(b"shdr", shdr))
open(out + "/sine.sf2", "wb").write(chunk(b"RIFF", body))

# Format 0, 480 ticks a beat at the default 120 bpm: 960 ticks are a second.
track = bytes([0x00, 0xC0, 0x00, 0x00, 0x90, 69, 100]) + bytes([0x87, 0x40, 0x80, 69, 64]) + bytes([0x00, 0xFF, 0x2F, 0x00])
mid = b"MThd" + struct.pack(">IHHH", 6, 0, 1, 480) + b"MTrk" + struct.pack(">I", len(track)) + track
open(out + "/a4.mid", "wb").write(mid)
