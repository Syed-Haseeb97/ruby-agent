import subprocess
import time
import os
import glob
import ctypes
from ctypes import wintypes

user32 = ctypes.windll.user32
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

def get_title(hwnd):
    buf = ctypes.create_unicode_buffer(512)
    user32.GetWindowTextW(hwnd, buf, 512)
    return buf.value

found_windows = {}
def enum_proc(hwnd, lParam):
    title = get_title(hwnd)
    if title in ("Ruby Orb", "Ruby Assistant Console"):
        found_windows[title] = hwnd
    return True

EnumProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

fg_before = user32.GetForegroundWindow() or 0
fg_before_title = get_title(fg_before)
print(f"Foreground window before launch: HWND=0x{fg_before:08X} ('{fg_before_title}')")

exe = r"D:\projects\ruby-agent\target\debug\ruby-desktop.exe"
proc = subprocess.Popen([exe], cwd=r"D:\projects\ruby-agent\crates\ruby-desktop")

try:
    time.sleep(1.5)
    user32.EnumWindows(EnumProc(enum_proc), 0)
    orb_hwnd = found_windows.get("Ruby Orb")
    console_hwnd = found_windows.get("Ruby Assistant Console")

    print(f"Ruby Orb HWND: 0x{orb_hwnd:08X}" if orb_hwnd else "Ruby Orb HWND: Not found")
    print(f"Console HWND: 0x{console_hwnd:08X}" if console_hwnd else "Console HWND: Not found")

    if orb_hwnd:
        ex = user32.GetWindowLongW(orb_hwnd, GWL_EXSTYLE)
        print(f"Ruby Orb ExStyle: 0x{ex:08X}")
        print(f"  WS_EX_TOOLWINDOW: {bool(ex & WS_EX_TOOLWINDOW)}")
        print(f"  WS_EX_NOACTIVATE: {bool(ex & WS_EX_NOACTIVATE)}")
        print(f"  WS_EX_LAYERED: {bool(ex & WS_EX_LAYERED)}")
        print(f"  WS_EX_TRANSPARENT: {bool(ex & WS_EX_TRANSPARENT)}")

        aff = wintypes.DWORD(0)
        user32.GetWindowDisplayAffinity(orb_hwnd, ctypes.byref(aff))
        print(f"  WDA_EXCLUDEFROMCAPTURE: 0x{aff.value:08X} -> {aff.value == WDA_EXCLUDEFROMCAPTURE}")

        fg_after = user32.GetForegroundWindow() or 0
        fg_after_title = get_title(fg_after)
        print(f"Foreground window while Orb active: HWND=0x{fg_after:08X} ('{fg_after_title}')")
        print(f"Focus stolen by Orb: {fg_after == orb_hwnd}")

    # Wait for snapshots to be written by the WebView2 canvas
    for _ in range(35):
        time.sleep(0.2)
        files = glob.glob(r"D:\projects\ruby-agent\acceptance\tauri_rendered\*.png")
        if len(files) >= 11:
            break

    files = glob.glob(r"D:\projects\ruby-agent\acceptance\tauri_rendered\*.png")
    print(f"Captured {len(files)} snapshots from live Tauri WebView2 canvas:")
    for f in sorted(files):
        print("  -", os.path.basename(f), f"({os.path.getsize(f)} bytes)")

finally:
    proc.terminate()
    proc.wait()
    print("Process terminated.")
