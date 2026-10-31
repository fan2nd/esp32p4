"""Send a quiet 440 Hz tone directly to the P4 Windows audio endpoint.

Python standard library only; does not change the system default audio device.
"""
import argparse
import ctypes as c
import math
import struct
import time
from ctypes import wintypes as w


class Caps(c.Structure):
    _fields_ = [("mid", w.WORD), ("pid", w.WORD), ("version", w.DWORD),
                ("name", w.WCHAR * 32), ("formats", w.DWORD),
                ("channels", w.WORD), ("reserved", w.WORD), ("support", w.DWORD)]


class Format(c.Structure):
    _pack_ = 2
    _fields_ = [("tag", w.WORD), ("channels", w.WORD), ("rate", w.DWORD),
                ("bytes_per_second", w.DWORD), ("align", w.WORD),
                ("bits", w.WORD), ("extra", w.WORD)]


class Header(c.Structure):
    _fields_ = [("data", c.c_void_p), ("length", w.DWORD), ("recorded", w.DWORD),
                ("user", c.c_size_t), ("flags", w.DWORD), ("loops", w.DWORD),
                ("next", c.c_void_p), ("reserved", c.c_size_t)]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seconds", type=float, default=3)
    parser.add_argument("--device", default="P4 USB", help="Audio endpoint name substring")
    args = parser.parse_args()
    if not 0 < args.seconds <= 30:
        parser.error("seconds must be in (0, 30]")
    api = c.WinDLL("winmm")
    devices = []
    for index in range(api.waveOutGetNumDevs()):
        caps = Caps()
        if api.waveOutGetDevCapsW(index, c.byref(caps), c.sizeof(caps)) == 0:
            print(index, caps.name, flush=True)
            if args.device in caps.name:
                devices.append(index)
    if len(devices) != 1:
        raise SystemExit(f"Expected one P4 output, found {devices}")
    handle = c.c_void_p()
    fmt = Format(1, 1, 16000, 32000, 2, 16, 0)
    result = api.waveOutOpen(c.byref(handle), devices[0], c.byref(fmt), 0, 0, 0)
    if result:
        raise SystemExit(f"waveOutOpen failed: {result}")
    # 5% amplitude with short fades avoids a loud tone or a click.
    count = int(16000 * args.seconds)
    pcm = b"".join(struct.pack("<h", int(1600 * min(1, i / 160, (count - 1 - i) / 160)
                                            * math.sin(2 * math.pi * 440 * i / 16000)))
                   for i in range(count))
    buffer = c.create_string_buffer(pcm)
    header = Header(c.cast(buffer, c.c_void_p), len(pcm), 0, 0, 0, 0, None, 0)
    prepared = False
    try:
        result = api.waveOutPrepareHeader(handle, c.byref(header), c.sizeof(header))
        if result:
            raise RuntimeError(f"waveOutPrepareHeader failed: {result}")
        prepared = True
        result = api.waveOutWrite(handle, c.byref(header), c.sizeof(header))
        if result:
            raise RuntimeError(f"waveOutWrite failed: {result}")
        deadline = time.monotonic() + args.seconds + 10
        while not header.flags & 1:
            if time.monotonic() > deadline:
                raise TimeoutError("P4 playback did not complete")
            time.sleep(0.05)
        print("Host playback completed; confirm sound on the physical speaker.")
    finally:
        api.waveOutReset(handle)
        if prepared:
            api.waveOutUnprepareHeader(handle, c.byref(header), c.sizeof(header))
        api.waveOutClose(handle)
