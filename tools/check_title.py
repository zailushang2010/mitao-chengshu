import ctypes
from ctypes import wintypes

user32 = ctypes.windll.user32
WNDENUMPROC = ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)


@WNDENUMPROC
def cb(hwnd, lparam):
    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    if user32.IsWindowVisible(hwnd):
        buf = ctypes.create_unicode_buffer(512)
        user32.GetWindowTextW(hwnd, buf, 512)
        title = buf.value
        if title and ("suiji" in title.lower() or "PotPlayer" in title or "今日" in title or "片单" in title):
            print("hwnd", int(hwnd), "pid", pid.value)
            print("title_repr", repr(title))
            print("codepoints", [hex(ord(c)) for c in title])
    return True


user32.EnumWindows(cb, 0)
