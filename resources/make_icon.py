# Draws the Lifestream icon: the Turks orb in summon-materia red, at 1024 px.
# Usage: python3 make_icon.py [radius as a fraction of the size] [out.png]
# Sizes up to 48 px use radius 0.44 so the orb fills more of the frame.
import math, numpy as np
from PIL import Image, ImageFilter
S = 1024
cx = cy = S / 2
import sys
R = S * float(sys.argv[1]) if len(sys.argv) > 1 else S * 0.40
y, x = np.mgrid[0:S, 0:S].astype(np.float32)
dx, dy = (x - cx) / R, (y - cy) / R
d = np.sqrt(dx * dx + dy * dy)
inside = d <= 1.0

def mix(a, b, t):
    t = np.clip(t, 0, 1)[..., None]
    return np.array(a, np.float32) * (1 - t) + np.array(b, np.float32) * t

# Body: deep violet rim -> bright magenta-violet core, lit from upper-left.
lx, ly = -0.35, -0.40
light = np.clip(1 - np.sqrt((dx - lx) ** 2 + (dy - ly) ** 2) / 1.6, 0, 1)
col = mix((92, 4, 10), (214, 26, 34), light ** 1.1)
col = mix(col, (255, 92, 78), np.clip(light - 0.45, 0, 1) * 1.8)

# Inner glowing core, slightly below centre, and a soft swirl band.
core = np.exp(-(((dx - 0.05) ** 2 + (dy - 0.12) ** 2) / 0.10))
col = mix(col, (255, 206, 176), core * 0.95)
ang = np.arctan2(dy, dx)
swirl = np.exp(-((d - (0.55 + 0.12 * np.sin(ang * 2 + 0.6))) ** 2) / 0.006) * (0.5 + 0.5 * np.sin(ang * 3 - d * 6))
col = mix(col, (255, 140, 112), np.clip(swirl, 0, 1) * 0.45)

# Rim darkening and a thin bright edge (glass).
col = mix(col, (42, 0, 4), np.clip((d - 0.82) / 0.18, 0, 1) ** 1.6 * 0.6)
edge = np.exp(-((d - 0.985) ** 2) / 0.00018)
col = mix(col, (255, 150, 128), edge * 0.55)

# Specular highlights: a big soft one upper-left and a small sharp one.
spec = np.exp(-(((dx + 0.36) / 0.30) ** 2 + ((dy + 0.44) / 0.17) ** 2))
col = mix(col, (255, 255, 255), spec * 0.75)
spec2 = np.exp(-(((dx + 0.42) / 0.07) ** 2 + ((dy + 0.48) / 0.05) ** 2))
col = mix(col, (255, 255, 255), spec2 * 0.95)
# Faint reflected light at the bottom-right.
refl = np.exp(-(((dx - 0.40) / 0.30) ** 2 + ((dy - 0.55) / 0.12) ** 2))
col = mix(col, (255, 172, 150), refl * 0.35)

alpha = np.clip((1.0 - d) * R / 1.2, 0, 1)  # anti-aliased edge
orb = np.dstack([col, alpha[..., None] * 255]).astype(np.uint8)
orb_img = Image.fromarray(orb, "RGBA")

# Outer glow behind the orb.
glow = np.exp(-np.clip(d - 1.0, 0, None) ** 2 / 0.018) * (d > 0.9)
g = np.zeros((S, S, 4), np.uint8)
g[..., 0], g[..., 1], g[..., 2] = 255, 60, 36
g[..., 3] = (glow * 150).astype(np.uint8)
out = Image.fromarray(g, "RGBA").filter(ImageFilter.GaussianBlur(14))
out.alpha_composite(orb_img)
out.save(sys.argv[2] if len(sys.argv) > 2 else "orb_1024.png")
