"""Test PrintWindow capture of the live Tauri Orb window."""
import ctypes
from ctypes import wintypes
import os
import subprocess
import time
from PIL import Image

user32 = ctypes.windll.user32
gdi32 = ctypes.windll.gdi32

PW_RENDERFULLCONTENT = 2
SRCCOPY = 0x00CC0020

def get_window_text(hwnd):
    buf = ctypes.create_unicode_buffer(512)
    user32.GetWindowTextW(hwnd, buf, 512)
    return buf.value

found_hwnd = None

def enum_proc(hwnd, lParam):
    global found_hwnd
    if get_window_text(hwnd) == "Ruby Orb":
        found_hwnd = hwnd
        return False
    return True

EnumProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

exe_path = r"D:\projects\ruby-agent\target\debug\ruby-desktop.exe"
proc = subprocess.Popen([exe_path], cwd=r"D:\projects\ruby-agent\crates\ruby-desktop")

try:
    time.sleep(3.5)
    user32.EnumWindows(EnumProc(enum_proc), 0)
    print(f"Found Ruby Orb HWND: 0x{found_hwnd:08X}")

    rect = wintypes.RECT()
    user32.GetClientRect(found_hwnd, ctypes.byref(rect))
    w = rect.right - rect.left
    h = rect.bottom - rect.top
    print(f"Client rect: {w}x{h}")

    hwnd_dc = user32.GetDC(found_hwnd)
    mem_dc = gdi32.CreateCompatibleDC(hwnd_dc)
    hbitmap = gdi32.CreateCompatibleBitmap(hwnd_dc, w, h)
    gdi32.SelectObject(mem_dc, hbitmap)

    # Use PrintWindow with PW_RENDERFULLCONTENT
    res = user32.PrintWindow(found_hwnd, mem_dc, PW_RENDERFULLCONTENT)
    print(f"PrintWindow result: {res}")

    # Extract bitmap bytes
    bmi = wintypes.BITMAPINFO() if hasattr(wintypes, 'BITMAPINFO') else None
    class BITMAPINFOHEADER(ctypes.Structure):
        _fields_ = [
            ("biSize", wintypes.DWORD),
            ("biWidth", wintypes.LONG),
            ("biHeight", wintypes.LONG),
            ("biPlanes", wintypes.WORD),
            ("biBitCount", wintypes.WORD),
            ("biCompression", wintypes.DWORD),
            ("biSizeImage", wintypes.DWORD),
            ("biXPelsPerMeter", wintypes.LONG),
            ("biYPelsPerMeter", wintypes.LONG),
            ("biClrUsed", wintypes.DWORD),
            ("biClrImportant", wintypes.DWORD),
        ]
    class BITMAPINFO(ctypes.Structure):
        _fields_ = [("bmiHeader", BITMAPINFOHEADER)]

    bi = BITMAPINFO()
    bi.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    bi.bmiHeader.biWidth = w
    bi.bmiHeader.biHeight = -h  # top-down
    bi.bmiHeader.biPlanes = 1
    bi.bmiHeader.biBitCount = 32
    bi.bmiHeader.biCompression = 0  # BI_RGB

    buffer = ctypes.create_string_buffer(w * h * 4)
    gdi32.GetDIBits(mem_dc, hbitmap, 0, h, buffer, ctypes.byref(bi), 0)

    img = Image.frombuffer("RGBA", (w, h), buffer.raw, "raw", "BGRA", 0, 1)
    out_path = r"D:\projects\ruby-agent\acceptance\live_tauri_orb_idle.png"
    img.save(out_path)
    print(f"Saved live captured Tauri orb image to: {out_path}")

    # Clean up GDI
    gdi32.DeleteObject(hbitmap)
    gdi32.DeleteDC(mem_dc)
    user32.ReleaseDC(found_hwnd, hwnd_dc)

finally:
    proc.terminate()
    proc.wait()
    print("Process finished.")
