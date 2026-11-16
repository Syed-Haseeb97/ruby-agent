import os
import sys
import math
import numpy as np
from PIL import Image, ImageChops, ImageDraw
from skimage.metrics import structural_similarity as ssim

sys.path.append(r"D:\projects\ai_agent")
from PyQt6.QtWidgets import QApplication
from PyQt6.QtGui import QImage, QPainter
from ui.liquid_blob import LiquidBlob, Mood

app = QApplication(sys.argv)

BASE_DIR = r"D:\projects\ruby-agent\acceptance"
PY_REF_DIR = os.path.join(BASE_DIR, "python_reference")
CANVAS_DIR = os.path.join(BASE_DIR, "canvas_rendered")
os.makedirs(PY_REF_DIR, exist_ok=True)
os.makedirs(CANVAS_DIR, exist_ok=True)

MOOD_MAP = [
    ("idle", Mood.IDLE, False),
    ("listening", Mood.LISTENING, False),
    ("thinking", Mood.THINKING, False),
    ("speaking", Mood.SPEAKING, True),
    ("happy", Mood.HAPPY, False),
    ("excited", Mood.EXCITED, False),
    ("sad", Mood.SAD, False),
    ("empathetic", Mood.EMPATHETIC, False),
    ("curious", Mood.CURIOUS, False),
    ("surprised", Mood.SURPRISED, False),
    ("error", Mood.ERROR, False),
]

print("Rendering deterministic PyQt6 references at t=0.0...")
blob = LiquidBlob()
blob.resize(300, 280)
# Mock _phase to return exactly 0.0
blob._phase = lambda: 0.0
blob._spin_angle = 0.0

for name, mood, speaking in MOOD_MAP:
    blob.set_mood(mood)
    blob.set_speaking_active(speaking)
    blob._spin_angle = 0.0
    
    img = QImage(300, 280, QImage.Format.Format_ARGB32_Premultiplied)
    img.fill(0)
    painter = QPainter(img)
    blob.render(painter)
    painter.end()
    
    out_path = os.path.join(PY_REF_DIR, f"{name}.png")
    img.save(out_path)
    print(f"Saved PyQt reference at t=0.0: {out_path}")

print("PyQt references saved successfully.")
