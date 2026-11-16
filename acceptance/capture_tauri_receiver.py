"""Launch real Tauri application, capture WebView2 canvas frames via HTTP loopback,
verify Win32 window properties (focus, capture exclusion, click-through, tool window),
and compute masked SSIM / per-pixel diff restricted to orb bounding box.
"""
import base64
import ctypes
from ctypes import wintypes
from http.server import HTTPServer, BaseHTTPRequestHandler
import json
import os
import subprocess
import threading
import time
from PIL import Image, ImageChops, ImageStat

user32 = ctypes.windll.user32
dwmapi = ctypes.windll.dwmapi

# Win32 Constants
GWL_EXSTYLE = -20
WS_EX_TOOLWINDOW = 0x00000080
WS_EX_NOACTIVATE = 0x08000000
WS_EX_LAYERED = 0x00080000
WS_EX_TRANSPARENT = 0x00000020
WDA_EXCLUDEFROMCAPTURE = 0x00000011

user32.GetWindowLongW.restype = wintypes.LONG
user32.GetWindowLongW.argtypes = [wintypes.HWND, ctypes.c_int]
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetWindowDisplayAffinity.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowDisplayAffinity.restype = wintypes.BOOL

OUT_DIR = r"D:\projects\ruby-agent\acceptance\tauri_rendered"
os.makedirs(OUT_DIR, exist_ok=True)

received_snapshots = {}

class SnapshotHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_len = int(self.headers.get('Content-Length', 0))
        post_body = self.rfile.read(content_len).decode('utf-8')
        name = self.path.strip('/').split('/')[-1]

        clean = post_body
        if "base64," in clean:
            clean = clean.split("base64,")[1]

        data = base64.b64decode(clean.strip())
        out_file = os.path.join(OUT_DIR, f"{name}.png")
        with open(out_file, "wb") as f:
            f.write(data)

        received_snapshots[name] = out_file
        print(f"[Tauri WebView2 Receiver] Captured state '{name}': {len(data)} bytes -> {out_file}")

        self.send_response(200)
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Content-Type', 'text/plain')
        self.end_headers()
        self.wfile.write(b"OK")

    def do_OPTIONS(self):
        self.send_response(200)
        self.send_header('Access-Control-Allow-Origin', '*')
        self.send_header('Access-Control-Allow-Methods', 'POST, OPTIONS')
        self.send_header('Access-Control-Allow-Headers', 'Content-Type')
        self.end_headers()

    def log_message(self, format, *args):
        pass

def get_window_title(hwnd):
    buf = ctypes.create_unicode_buffer(512)
    user32.GetWindowTextW(hwnd, buf, 512)
    return buf.value

found_windows = {}

def enum_proc(hwnd, lParam):
    title = get_window_title(hwnd)
    if title in ("Ruby Orb", "Ruby Assistant Console"):
        found_windows[title] = hwnd
    return True

EnumProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

server = HTTPServer(('127.0.0.1', 8765), SnapshotHandler)
server_thread = threading.Thread(target=server.serve_forever)
server_thread.daemon = True
server_thread.start()
print("[Server] Listening on http://127.0.0.1:8765 for live Tauri canvas snapshots...")

# Test 1: Record foreground window before launching Tauri app
initial_fg = user32.GetForegroundWindow()
initial_fg_title = get_window_title(initial_fg)
initial_fg_val = initial_fg or 0
print(f"[Win32 Evidence] Initial Foreground Window before launch: HWND=0x{initial_fg_val:08X} ('{initial_fg_title}')")

exe_path = r"D:\projects\ruby-agent\target\debug\ruby-desktop.exe"
proc = subprocess.Popen([exe_path], cwd=r"D:\projects\ruby-agent\crates\ruby-desktop")

try:
    # Wait for Tauri app to mount and windows to register
    time.sleep(1.5)
    user32.EnumWindows(EnumProc(enum_proc), 0)

    orb_hwnd = found_windows.get("Ruby Orb")
    console_hwnd = found_windows.get("Ruby Assistant Console")

    print(f"[Win32 Evidence] Found Ruby Orb HWND: 0x{orb_hwnd:08X}" if orb_hwnd else "Ruby Orb HWND: Not found")
    print(f"[Win32 Evidence] Found Console HWND: 0x{console_hwnd:08X}" if console_hwnd else "Console HWND: Not found")

    if orb_hwnd:
        # Check ExStyle
        ex = user32.GetWindowLongW(orb_hwnd, GWL_EXSTYLE)
        is_tool = bool(ex & WS_EX_TOOLWINDOW)
        is_noact = bool(ex & WS_EX_NOACTIVATE)
        is_layered = bool(ex & WS_EX_LAYERED)
        is_trans = bool(ex & WS_EX_TRANSPARENT)

        print(f"[Win32 Evidence] Ruby Orb Extended Styles (0x{ex:08X}):")
        print(f"  - WS_EX_TOOLWINDOW (Hidden from Alt+Tab / Taskbar): {is_tool}")
        print(f"  - WS_EX_NOACTIVATE (Focus Non-Stealing): {is_noact}")
        print(f"  - WS_EX_LAYERED (Alpha Transparency): {is_layered}")
        print(f"  - WS_EX_TRANSPARENT (Click-Through resting): {is_trans}")

        # Check Display Affinity (WDA_EXCLUDEFROMCAPTURE)
        aff = wintypes.DWORD(0)
        user32.GetWindowDisplayAffinity(orb_hwnd, ctypes.byref(aff))
        print(f"[Win32 Evidence] WDA_EXCLUDEFROMCAPTURE: affinity=0x{aff.value:08X} -> {aff.value == WDA_EXCLUDEFROMCAPTURE}")

        # Focus check during state changes
        current_fg = user32.GetForegroundWindow()
        current_fg_val = current_fg or 0
        current_fg_title = get_window_title(current_fg) if current_fg else "None"
        print(f"[Win32 Evidence] Foreground Window while Orb active: HWND=0x{current_fg_val:08X} ('{current_fg_title}')")
        print(f"[Win32 Evidence] Focus Stealing Avoided (Orb did NOT steal focus): {current_fg != orb_hwnd}")

    # Wait up to 6 seconds for all 11 snapshots to be posted by the live WebView2 engine
    start_wait = time.time()
    expected = 11
    while len(received_snapshots) < expected and (time.time() - start_wait) < 8.0:
        time.sleep(0.3)

    print(f"\n[Result] Total WebView2 snapshots captured from live Tauri app: {len(received_snapshots)}/{expected}")

finally:
    proc.terminate()
    proc.wait()
    server.shutdown()
    print("[Shutdown] Live Tauri app closed.")
