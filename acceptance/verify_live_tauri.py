"""Verify real live Tauri 2.0 app windows and Win32 properties."""
import ctypes
from ctypes import wintypes
import os
import subprocess
import time

user32 = ctypes.windll.user32
dwmapi = ctypes.windll.dwmapi

# Win32 Constants
GWL_EXSTYLE = -20
WS_EX_TOOLWINDOW = 0x00000080
WS_EX_NOACTIVATE = 0x08000000
WS_EX_LAYERED = 0x00080000
WS_EX_TRANSPARENT = 0x00000020
WDA_NONE = 0x00000000
WDA_MONITOR = 0x00000001
WDA_EXCLUDEFROMCAPTURE = 0x00000011
DWMWA_NCRENDERING_POLICY = 2
DWMNCRP_USEWINDOWSTYLE = 0
DWMNCRP_DISABLED = 1
DWMNCRP_ENABLED = 2

# Prototypes
user32.GetWindowLongW.restype = wintypes.LONG
user32.GetWindowLongW.argtypes = [wintypes.HWND, ctypes.c_int]
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetWindowDisplayAffinity.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowDisplayAffinity.restype = wintypes.BOOL
dwmapi.DwmGetWindowAttribute.argtypes = [wintypes.HWND, wintypes.DWORD, ctypes.c_void_p, wintypes.DWORD]
dwmapi.DwmGetWindowAttribute.restype = ctypes.c_long

def get_window_text(hwnd):
    buf = ctypes.create_unicode_buffer(512)
    user32.GetWindowTextW(hwnd, buf, 512)
    return buf.value

def get_class_name(hwnd):
    buf = ctypes.create_unicode_buffer(512)
    user32.GetClassNameW(hwnd, buf, 512)
    return buf.value

found_windows = []

def enum_windows_proc(hwnd, lParam):
    title = get_window_text(hwnd)
    if "Ruby" in title or "Ruby Orb" in title:
        found_windows.append((hwnd, title, get_class_name(hwnd)))
    return True

EnumWindowsProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)

print("Starting ruby-desktop.exe...")
exe_path = r"D:\projects\ruby-agent\target\debug\ruby-desktop.exe"
proc = subprocess.Popen([exe_path], cwd=r"D:\projects\ruby-agent\crates\ruby-desktop")

try:
    # Wait for windows to initialize
    time.sleep(3.5)

    user32.EnumWindows(EnumWindowsProc(enum_windows_proc), 0)
    print(f"Discovered matching windows: {len(found_windows)}")
    for hwnd, title, cls in found_windows:
        print(f"  HWND: 0x{hwnd:08X} | Title: '{title}' | Class: '{cls}'")

        ex_style = user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
        has_tool = bool(ex_style & WS_EX_TOOLWINDOW)
        has_noact = bool(ex_style & WS_EX_NOACTIVATE)
        has_layered = bool(ex_style & WS_EX_LAYERED)
        has_transparent = bool(ex_style & WS_EX_TRANSPARENT)

        affinity = wintypes.DWORD(0)
        aff_ok = user32.GetWindowDisplayAffinity(hwnd, ctypes.byref(affinity))

        policy = ctypes.c_int(0)
        dwm_ok = dwmapi.DwmGetWindowAttribute(hwnd, DWMWA_NCRENDERING_POLICY, ctypes.byref(policy), ctypes.sizeof(policy))

        print(f"    EX_STYLE: 0x{ex_style:08X}")
        print(f"      WS_EX_TOOLWINDOW: {has_tool}")
        print(f"      WS_EX_NOACTIVATE: {has_noact}")
        print(f"      WS_EX_LAYERED: {has_layered}")
        print(f"      WS_EX_TRANSPARENT: {has_transparent}")
        print(f"    GetWindowDisplayAffinity: OK={bool(aff_ok)}, Value=0x{affinity.value:08X} (WDA_EXCLUDEFROMCAPTURE: {affinity.value == WDA_EXCLUDEFROMCAPTURE})")
        print(f"    DWMWA_NCRENDERING_POLICY: OK={dwm_ok == 0}, Value={policy.value} (DWMNCRP_DISABLED: {policy.value == DWMNCRP_DISABLED})")

finally:
    print("Terminating ruby-desktop.exe...")
    proc.terminate()
    proc.wait()
    print("Terminated.")
