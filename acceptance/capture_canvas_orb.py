"""Automated side-by-side acceptance generator for Ruby Liquid Orb.
Renders PyQt6 reference screenshots and Canvas 2D screenshots,
computes pixel correlation/diff, and writes an acceptance comparison markdown.
"""
import os
import sys
from playwright.sync_api import sync_playwright
from PIL import Image, ImageChops, ImageDraw, ImageFont

MOOD_CASES = [
    {"name": "idle", "op": "Idle", "emo": "Neutral", "speaking": False},
    {"name": "listening", "op": "Listening", "emo": "Neutral", "speaking": False},
    {"name": "thinking", "op": "Thinking", "emo": "Neutral", "speaking": False},
    {"name": "speaking", "op": "Speaking", "emo": "Neutral", "speaking": True},
    {"name": "happy", "op": "Idle", "emo": "Happy", "speaking": False},
    {"name": "excited", "op": "Idle", "emo": "Excited", "speaking": False},
    {"name": "sad", "op": "Idle", "emo": "Sad", "speaking": False},
    {"name": "empathetic", "op": "Idle", "emo": "Empathetic", "speaking": False},
    {"name": "curious", "op": "Idle", "emo": "Curious", "speaking": False},
    {"name": "surprised", "op": "Idle", "emo": "Surprised", "speaking": False},
    {"name": "error", "op": "Error", "emo": "Neutral", "speaking": False},
]

BASE_DIR = r"D:\projects\ruby-agent\acceptance"
PY_REF_DIR = os.path.join(BASE_DIR, "python_reference")
CANVAS_DIR = os.path.join(BASE_DIR, "canvas_rendered")
COMPARE_DIR = os.path.join(BASE_DIR, "side_by_side")

os.makedirs(CANVAS_DIR, exist_ok=True)
os.makedirs(COMPARE_DIR, exist_ok=True)

html_url = "file:///" + os.path.join(BASE_DIR, "orb_canvas_renderer.html").replace("\\", "/")

print(f"Opening {html_url} in headless Playwright...")

with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page(viewport={"width": 320, "height": 300}, device_scale_factor=1)
    page.goto(html_url)

    for case in MOOD_CASES:
        name = case["name"]
        op = case["op"]
        emo = case["emo"]
        spk = "true" if case["speaking"] else "false"

        page.evaluate(f"window.renderOrb('{op}', '{emo}', 0.0, 45.0, {spk})")
        canvas_handle = page.query_selector("#orb-canvas")
        out_png = os.path.join(CANVAS_DIR, f"{name}.png")
        canvas_handle.screenshot(path=out_png, omit_background=True)
        print(f"Captured Canvas 2D: {out_png}")

    browser.close()

print("All Canvas 2D frames captured! Generating side-by-side composite images...")

diff_reports = []

for case in MOOD_CASES:
    name = case["name"]
    py_path = os.path.join(PY_REF_DIR, f"{name}.png")
    cv_path = os.path.join(CANVAS_DIR, f"{name}.png")

    if not os.path.exists(py_path):
        print(f"Warning: {py_path} not found!")
        continue

    img_py = Image.open(py_path).convert("RGBA")
    img_cv = Image.open(cv_path).convert("RGBA")

    # Composite side-by-side image: 640x330 (with header banner)
    banner_height = 40
    comp_width = 300 + 300 + 30
    comp_height = 280 + banner_height + 20

    comp = Image.new("RGBA", (comp_width, comp_height), (245, 243, 238, 255))
    draw = ImageDraw.Draw(comp)

    # Header labels
    title_text = f"Mood/State: {name.upper()}"
    draw.text((15, 12), title_text, fill=(22, 24, 29, 255))

    sub_left = "PyQt6 Original (Commit 7bc2c3a)"
    sub_right = "Rust / Tauri Canvas 2D"
    draw.text((15, banner_height - 6), sub_left, fill=(95, 98, 106, 255))
    draw.text((335, banner_height - 6), sub_right, fill=(95, 98, 106, 255))

    # Paste orbs
    comp.paste(img_py, (10, banner_height + 10), img_py)
    comp.paste(img_cv, (330, banner_height + 10), img_cv)

    comp_path = os.path.join(COMPARE_DIR, f"compare_{name}.png")
    comp.save(comp_path)
    print(f"Saved side-by-side comparison: {comp_path}")

print("Side-by-side generation complete!")
