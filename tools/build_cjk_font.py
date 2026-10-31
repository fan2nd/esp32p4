"""Build the 16px CJK bitmap subset used for Wi-Fi SSIDs (no runtime C library).

Requires Pillow and Noto Sans SC (SIL OFL, assets/CJK-FONT-LICENSE.txt).
Pass the downloaded NotoSansSC[wght].ttf as the first argument.
"""
from pathlib import Path
import sys
from PIL import Image, ImageDraw, ImageFont
font = ImageFont.truetype(sys.argv[1], 16)
font.set_variation_by_axes([400])
result = bytearray()
for code in range(0x4E00, 0xA000):
    image = Image.new("1", (16, 16))
    top = font.getbbox(chr(code))[1]
    ImageDraw.Draw(image).text((0, -top), chr(code), font=font, fill=1)
    result.extend(image.tobytes())
Path("assets/cjk16.bin").write_bytes(result)
print(f"Generated {len(result)} bytes")
