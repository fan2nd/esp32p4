"""Capture a short PCM16 sample from the P4 microphone; report levels, save nothing."""
import argparse
import ctypes as c
import math
import struct
import time
from ctypes import wintypes as w
from usb_audio_playback import Format, Header


class InputCaps(c.Structure):
    _fields_ = [("mid", w.WORD), ("pid", w.WORD), ("version", w.DWORD),
                ("name", w.WCHAR * 32), ("formats", w.DWORD),
                ("channels", w.WORD), ("reserved", w.WORD)]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--device", default="P4 Desktop")
    parser.add_argument("--seconds", type=float, default=3)
    args = parser.parse_args()
    if not 0 < args.seconds <= 30:
        parser.error("seconds must be in (0, 30]")
    api = c.WinDLL("winmm")
    matches = []
    for index in range(api.waveInGetNumDevs()):
        caps = InputCaps()
        if api.waveInGetDevCapsW(index, c.byref(caps), c.sizeof(caps)) == 0:
            print(index, caps.name, flush=True)
            if args.device in caps.name:
                matches.append(index)
    if len(matches) != 1:
        raise SystemExit(f"Expected one P4 microphone, found {matches}")
    handle = c.c_void_p()
    fmt = Format(1, 1, 16000, 32000, 2, 16, 0)
    result = api.waveInOpen(c.byref(handle), matches[0], c.byref(fmt), 0, 0, 0)
    if result:
        raise RuntimeError(f"waveInOpen failed: {result}")
    buffer = c.create_string_buffer(int(args.seconds * 16000) * 2)
    header = Header(c.cast(buffer, c.c_void_p), len(buffer), 0, 0, 0, 0, None, 0)
    prepared = False
    try:
        result = api.waveInPrepareHeader(handle, c.byref(header), c.sizeof(header))
        if result:
            raise RuntimeError(f"waveInPrepareHeader failed: {result}")
        prepared = True
        result = api.waveInAddBuffer(handle, c.byref(header), c.sizeof(header))
        if result:
            raise RuntimeError(f"waveInAddBuffer failed: {result}")
        result = api.waveInStart(handle)
        if result:
            raise RuntimeError(f"waveInStart failed: {result}")
        deadline = time.monotonic() + args.seconds + 10
        while not header.flags & 1:
            if time.monotonic() > deadline:
                raise TimeoutError("P4 microphone capture did not complete")
            time.sleep(0.05)
        samples = [v[0] for v in struct.iter_unpack("<h", buffer.raw[:header.recorded])]
        if not samples:
            raise RuntimeError("No microphone samples returned")
        mean = sum(samples) / len(samples)
        rms = math.sqrt(sum((s - mean) ** 2 for s in samples) / len(samples))
        peak = max(abs(s) for s in samples)
        print(f"Captured {len(samples)} samples; AC RMS={rms:.1f}, peak={peak}, "
              f"nonzero={sum(s != 0 for s in samples)}, clipped={sum(abs(s) >= 32767 for s in samples)}")
        print("Nonzero samples alone do not prove acoustic microphone response.")
    finally:
        api.waveInReset(handle)
        if prepared:
            api.waveInUnprepareHeader(handle, c.byref(header), c.sizeof(header))
        api.waveInClose(handle)
