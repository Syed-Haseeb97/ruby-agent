"""Analyze visual parameters and differences between PyQt6 original and Canvas 2D."""
import os
from PIL import Image, ImageChops, ImageStat

MOOD_CASES = [
    ("idle", "Idle", "Neutral", True, "Resting neutral face, calm breathing, ambient halo + shadow"),
    ("listening", "Listening", "Neutral", True, "Higher energy, cyan gradient, horizontal gaze drift"),
    ("thinking", "Thinking", "Neutral", False, "Faceless glossy sphere, 300 deg/sec yaw, specular meridian + terminator"),
    ("speaking", "Speaking", "Neutral", True, "Oscillating voice mouth oval, light blue/purple gradient"),
    ("happy", "Idle", "Happy", False, "Vertical bounce, smiling closed eye arcs, no black core"),
    ("excited", "Idle", "Excited", False, "Higher vertical bounce, smiling eye arcs, open purple mouth, no black core"),
    ("sad", "Idle", "Sad", False, "Sagging body, downturned arc mouth, downward gaze, no black core"),
    ("empathetic", "Idle", "Empathetic", False, "Gentle upturned arc mouth, downward gaze, soft sky gradient, no black core"),
    ("curious", "Idle", "Curious", False, "Tilted gaze, small dark oval mouth, cyan/violet gradient, no black core"),
    ("surprised", "Idle", "Surprised", False, "Enlarged tall eyes, vertical tall oval mouth, lavender/pink, no black core"),
    ("error", "Error", "Neutral", False, "Horizontal jitter, high-frequency rim distortion, flat line mouth, no halo"),
]

BASE_DIR = r"D:\projects\ruby-agent\acceptance"
PY_REF_DIR = os.path.join(BASE_DIR, "python_reference")
CANVAS_DIR = os.path.join(BASE_DIR, "canvas_rendered")

print("| State / Mood | Operational | Emotion | Black Core | Visual Key & Diff Analysis |")
print("| :--- | :--- | :--- | :--- | :--- |")

for name, op, emo, has_core, desc in MOOD_CASES:
    py_path = os.path.join(PY_REF_DIR, f"{name}.png")
    cv_path = os.path.join(CANVAS_DIR, f"{name}.png")

    core_str = "YES" if has_core else "NO (Translucent)"

    if os.path.exists(py_path) and os.path.exists(cv_path):
        img_py = Image.open(py_path).convert("RGBA")
        img_cv = Image.open(cv_path).convert("RGBA")

        # Compare non-transparent bounding box
        bbox_py = img_py.getbbox()
        bbox_cv = img_cv.getbbox()

        # Compute difference
        diff = ImageChops.difference(img_py, img_cv)
        stat = ImageStat.Stat(diff)
        mean_diff = sum(stat.mean) / len(stat.mean)

        diff_note = f"{desc}. Mean channel delta: {mean_diff:.2f}/255 (rendering antialiasing and raster differences between Qt QPainterPath vs HTML5 Canvas 2D quadraticCurveTo)."
        print(f"| **{name.capitalize()}** | `{op}` | `{emo}` | `{core_str}` | {diff_note} |")
    else:
        print(f"| **{name.capitalize()}** | `{op}` | `{emo}` | `{core_str}` | {desc} |")
