import os
import sys
import math
import numpy as np
from PIL import Image, ImageChops, ImageDraw
from playwright.sync_api import sync_playwright
from skimage.metrics import structural_similarity as ssim

sys.path.append(r"D:\projects\ai_agent")
from PyQt6.QtWidgets import QApplication
from PyQt6.QtGui import QImage, QPainter
from ui.liquid_blob import LiquidBlob, Mood

app = QApplication(sys.argv)

BASE_DIR = r"D:\projects\ruby-agent\acceptance"
MATRIX_DIR = os.path.join(BASE_DIR, "matrix")
os.makedirs(MATRIX_DIR, exist_ok=True)

MOOD_CASES = [
    ("idle", Mood.IDLE, "Idle", "Neutral", False),
    ("listening", Mood.LISTENING, "Listening", "Neutral", False),
    ("thinking", Mood.THINKING, "Thinking", "Neutral", False),
    ("speaking", Mood.SPEAKING, "Speaking", "Neutral", True),
    ("happy", Mood.HAPPY, "Idle", "Happy", False),
    ("excited", Mood.EXCITED, "Idle", "Excited", False),
    ("sad", Mood.SAD, "Idle", "Sad", False),
    ("empathetic", Mood.EMPATHETIC, "Idle", "Empathetic", False),
    ("curious", Mood.CURIOUS, "Idle", "Curious", False),
    ("surprised", Mood.SURPRISED, "Idle", "Surprised", False),
    ("error", Mood.ERROR, "Error", "Neutral", False),
]

TIMESTAMPS = [0.0, 1.7, 3.3]

# 1. Render PyQt6 Original References
print("Rendering PyQt6 Original references...")
blob = LiquidBlob()
blob.resize(300, 280)

for t in TIMESTAMPS:
    blob._phase = lambda t_val=t: t_val
    spin = (300.0 * t) % 360.0
    blob._spin_angle = spin
    for name, py_mood, op, emo, spk in MOOD_CASES:
        blob.set_mood(py_mood)
        blob.set_speaking_active(spk)
        blob._spin_angle = spin

        img = QImage(300, 280, QImage.Format.Format_ARGB32_Premultiplied)
        img.fill(0)
        painter = QPainter(img)
        blob.render(painter)
        painter.end()

        out_path = os.path.join(MATRIX_DIR, f"pyqt_t_{t}_{name}.png")
        img.save(out_path)

# 2. Render Canvas using shipped LiquidOrbCanvas.tsx
print("Rendering Canvas from shipped LiquidOrbCanvas.tsx via Playwright...")
html_path = "file:///" + os.path.join(BASE_DIR, "shipped_orb_test.html").replace("\\", "/")

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page(viewport={"width": 300, "height": 280}, device_scale_factor=1)
    page.goto(html_path)
    page.wait_for_function("() => window.orbReady === true")

    for t in TIMESTAMPS:
        spin = (300.0 * t) % 360.0
        for name, py_mood, op, emo, spk in MOOD_CASES:
            spk_str = "true" if spk else "false"
            page.evaluate(f"window.renderShippedOrb('{op}', '{emo}', {t}, {spin}, {spk_str})")
            canvas = page.query_selector("#orb-canvas")
            out_png = os.path.join(MATRIX_DIR, f"canvas_t_{t}_{name}.png")
            canvas.screenshot(path=out_png, omit_background=True)

    browser.close()

# 3. Compute Metrics for each t and state
print("\n" + "=" * 80)
print("FIDELITY MATRIX: PyQt6 Original vs Shipped LiquidOrbCanvas.tsx")
print("=" * 80)

results = {}

for t in TIMESTAMPS:
    print(f"\n### Results at Animation Time t = {t} s")
    print("| State / Emotion | Full SSIM | Masked SSIM (Orb Bounding Region) | Mean Delta (/255) | Max Delta | Fidelity Status |")
    print("| :--- | :---: | :---: | :---: | :---: | :--- |")

    for name, py_mood, op, emo, spk in MOOD_CASES:
        py_file = os.path.join(MATRIX_DIR, f"pyqt_t_{t}_{name}.png")
        cv_file = os.path.join(MATRIX_DIR, f"canvas_t_{t}_{name}.png")

        img_py = Image.open(py_file).convert("RGBA").crop((0, 0, 300, 280))
        img_cv = Image.open(cv_file).convert("RGBA").crop((0, 0, 300, 280))

        # Alpha composite on warm paper background #F5F3EE
        bg = Image.new("RGBA", (300, 280), (245, 243, 238, 255))
        py_comp = Image.alpha_composite(bg, img_py).convert("RGB")
        cv_comp = Image.alpha_composite(bg, img_cv).convert("RGB")

        arr_py = np.array(py_comp)
        arr_cv = np.array(cv_comp)

        # Full image SSIM
        full_ssim = ssim(arr_py, arr_cv, channel_axis=2, data_range=255)
        mean_delta = float(np.mean(np.abs(arr_py.astype(float) - arr_cv.astype(float))))
        max_delta = int(np.max(np.abs(arr_py.astype(int) - arr_cv.astype(int))))

        # Masked SSIM: Restricted to orb bounding region [y: 35..235, x: 45..255]
        # where the actual orb, halo, and shadow exist
        py_alpha = np.array(img_py.split()[-1])
        cv_alpha = np.array(img_cv.split()[-1])
        mask = (py_alpha > 5) | (cv_alpha > 5)

        # Bounding box of mask
        rows = np.any(mask, axis=1)
        cols = np.any(mask, axis=0)
        if np.any(rows) and np.any(cols):
            ymin, ymax = np.where(rows)[0][[0, -1]]
            xmin, xmax = np.where(cols)[0][[0, -1]]
            # Pad slightly
            ymin = max(0, ymin - 4)
            ymax = min(280, ymax + 5)
            xmin = max(0, xmin - 4)
            xmax = min(300, xmax + 5)
        else:
            ymin, ymax, xmin, xmax = 30, 240, 45, 255

        crop_py = arr_py[ymin:ymax, xmin:xmax]
        crop_cv = arr_cv[ymin:ymax, xmin:xmax]
        masked_ssim = ssim(crop_py, crop_cv, channel_axis=2, data_range=255)

        status = "PASSED (>0.90)" if masked_ssim >= 0.90 else "BELOW TARGET"
        print(f"| **{name.capitalize()}** | {full_ssim:.4f} | **{masked_ssim:.4f}** | {mean_delta:.2f} | {max_delta} | `{status}` |")

print("\nFidelity matrix complete.")
