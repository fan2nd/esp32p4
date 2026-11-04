"""Bake the MicroTS control font from an OFL Inter TTF (Pillow build tool only)."""
import argparse
import struct
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

parser = argparse.ArgumentParser()
parser.add_argument("font", type=Path)
args = parser.parse_args()
font = ImageFont.truetype(str(args.font), 16 * 2)
chars = list(range(32, 127)) + [176]
width, height, baseline = 20, 24, 18
blob = bytearray(struct.pack("<IHH8B", 0x41464344, 3, len(chars), width, height, baseline, 24, 2, 0, 2, 0))
images = []
for index, cp in enumerate(chars):
    char = chr(cp)
    advance = round(font.getlength(char) / 2)
    blob += struct.pack("<IHBB", cp, index, advance, 0)
    image = Image.new("L", (width * 2, height * 2))
    ImageDraw.Draw(image).text((0, baseline * 2), char, font=font, anchor="ls", fill=255)
    images.append(image.tobytes())
blob += b"".join(images)
output = Path(__file__).resolve().parents[1] / "assets/ui/pocket/inter16.bin"
output.write_bytes(blob)
print(output, len(blob))
