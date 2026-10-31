"""Test VIA transport without sending keyboard input. Requires pip install hidapi.

Default is read-only. --roundtrip temporarily changes L3's final key and restores it.
"""
import argparse
import time
import hid

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--roundtrip", action="store_true")
parser.add_argument("--pid", type=lambda value: int(value, 0), default=0x4013)
args = parser.parse_args()
devices = [d for d in hid.enumerate(0xCAFE, args.pid)
           if d["usage_page"] == 0xFF60 and d["usage"] == 0x61]
if len(devices) != 1:
    raise SystemExit(f"Expected one P4 VIA interface, found {len(devices)}")
device = hid.device()
device.open_path(devices[0]["path"])

def exchange(*values):
    packet = bytes(values).ljust(32, b"\0")
    device.write(b"\0" + packet)  # Windows HID API prepends the report ID.
    reply = bytes(device.read(32, 2000))
    if len(reply) != 32 or reply[0] != packet[0]:
        raise RuntimeError(f"VIA request {packet.hex()} failed: {reply.hex()}")
    return reply

try:
    assert exchange(1)[1:3] == b"\0\r"
    assert exchange(17)[1] == 4
    assert exchange(12)[1] == 8
    assert int.from_bytes(exchange(13)[1:3], "big") == 512
    for layer in range(4):
        codes = [int.from_bytes(exchange(4, layer, key // 2, key % 2)[4:6], "big")
                 for key in range(8)]
        print(f"L{layer}: " + " ".join(f"{c:04X}" for c in codes))
    print("VIA v13: 4 layers, 8 keys, 8 macros / 512 bytes; read test passed")
    if args.roundtrip:
        original = exchange(4, 3, 3, 1)[4:6]
        try:
            exchange(5, 3, 3, 1, 0, 0x45)
            assert exchange(4, 3, 3, 1)[4:6] == b"\0E"
            print("Remap readback passed")
        finally:
            exchange(5, 3, 3, 1, *original)
            assert exchange(4, 3, 3, 1)[4:6] == original
            time.sleep(2)
            print("Original key restored; persistence must also be checked after reboot")
finally:
    device.close()
