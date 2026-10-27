"""Render the fixed Chinese greeting to RGB888 for the display framebuffer."""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent.parent
SCALE = 2
image = Image.new("RGB", (440 * SCALE, 125 * SCALE), (232, 247, 255))
draw = ImageDraw.Draw(image)
ink = (38, 79, 134)
white = (255, 255, 255)
# Draw the tail first so the rounded bubble body covers its top edge.
draw.polygon([(250 * SCALE, 85 * SCALE), (285 * SCALE, 85 * SCALE),
              (269 * SCALE, 117 * SCALE)], fill=white, outline=ink, width=3 * SCALE)
draw.rounded_rectangle((3 * SCALE, 3 * SCALE, 437 * SCALE, 91 * SCALE),
                       radius=30 * SCALE, fill=white, outline=ink, width=3 * SCALE)
font = ImageFont.truetype("C:/Windows/Fonts/msyhbd.ttc", 34 * SCALE)
draw.text((220 * SCALE, 47 * SCALE), "你好，有大白饭吗", font=font,
          fill=ink, anchor="mm")
image = image.resize((440, 125), Image.Resampling.LANCZOS)
image.save(ROOT / "assets" / "pet-bubble.png")
(ROOT / "assets" / "pet-bubble.rgb").write_bytes(image.tobytes())
