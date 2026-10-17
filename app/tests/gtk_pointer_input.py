"""Real X11 input for the opt-in GTK regression test; no Python packages needed."""
import ctypes as c
import sys
import time

x = c.CDLL("libX11.so.6")
t = c.CDLL("libXtst.so.6")
D, W, I, U = c.c_void_p, c.c_ulong, c.c_int, c.c_uint
x.XOpenDisplay.argtypes, x.XOpenDisplay.restype = [c.c_char_p], D
x.XDefaultRootWindow.argtypes, x.XDefaultRootWindow.restype = [D], W
x.XTranslateCoordinates.argtypes = [D, W, W, I, I, c.POINTER(I), c.POINTER(I), c.POINTER(W)]
x.XQueryPointer.argtypes = [D, W, c.POINTER(W), c.POINTER(W), c.POINTER(I), c.POINTER(I), c.POINTER(I), c.POINTER(I), c.POINTER(U)]
x.XKeysymToKeycode.argtypes, x.XKeysymToKeycode.restype = [D, c.c_ulong], c.c_ubyte
x.XSetInputFocus.argtypes = [D, W, I, c.c_ulong]
x.XRaiseWindow.argtypes = [D, W]
x.XSync.argtypes = [D, I]
x.XCloseDisplay.argtypes = [D]
t.XTestFakeMotionEvent.argtypes = [D, I, I, I, c.c_ulong]
t.XTestFakeButtonEvent.argtypes = [D, U, I, c.c_ulong]
t.XTestFakeKeyEvent.argtypes = [D, U, I, c.c_ulong]
display = x.XOpenDisplay(None)
assert display, "X11 display required (run GTK test with GDK_BACKEND=x11)"
root = x.XDefaultRootWindow(display)

window = int(sys.argv[1])
rx, ry, wx, wy, mask, child, actual_root = I(), I(), I(), I(), U(), W(), W()
x.XQueryPointer(display, root, c.byref(actual_root), c.byref(child), c.byref(rx), c.byref(ry), c.byref(wx), c.byref(wy), c.byref(mask))
px, py = I(), I()
x.XTranslateCoordinates(display, window, root, int(sys.argv[2]), int(sys.argv[3]), c.byref(px), c.byref(py), c.byref(child))
try:
    x.XRaiseWindow(display, window)
    t.XTestFakeMotionEvent(display, -1, px.value, py.value, 0)
    x.XSync(display, 0)
    time.sleep(0.1)  # GTK processes enter/motion before the input below.
    action = sys.argv[4]
    if action == "wheel":
        for _ in range(3):
            t.XTestFakeButtonEvent(display, 5, 1, 0)
            t.XTestFakeButtonEvent(display, 5, 0, 0)
    elif action == "key":
        x.XSetInputFocus(display, window, 1, 0)
        key = x.XKeysymToKeycode(display, 0xFF53)  # Right
        t.XTestFakeKeyEvent(display, key, 1, 0)
        t.XTestFakeKeyEvent(display, key, 0, 0)
    elif action == "drag":
        t.XTestFakeButtonEvent(display, 1, 1, 0)
        x.XSync(display, 0)
        time.sleep(0.03)
        t.XTestFakeMotionEvent(display, -1, px.value + int(sys.argv[5]), py.value, 0)
        t.XTestFakeButtonEvent(display, 1, 0, 0)
    else:
        raise AssertionError(action)
    x.XSync(display, 0)
    time.sleep(0.1)
finally:
    t.XTestFakeMotionEvent(display, -1, rx.value, ry.value, 0)
    x.XSync(display, 0)
    x.XCloseDisplay(display)
