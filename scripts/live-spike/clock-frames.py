#!/usr/bin/env python3
"""Emit 640x360 rgb24 frames at 30 fps to stdout, each stamped with the UTC wall clock
(HH:MM:SS.mmm) drawn as big 7-segment digits. No PIL, no numpy, no drawtext needed.
Frames are paced to real time so the encoder sees a live source."""
import sys, time

W, H, FPS = 640, 360, 30
CW, CH, GAP = 40, 72, 8          # glyph cell size and spacing
TEXT_LEN = 12                    # HH:MM:SS.mmm
X0 = (W - TEXT_LEN * (CW + GAP)) // 2
Y0 = (H - CH) // 2
BG = bytes((24, 24, 40))
FG = bytes((255, 255, 255))
BAR = bytes((0, 200, 120))

# 7-segment layout: a top, b top-right, c bottom-right, d bottom, e bottom-left, f top-left, g middle
SEG = {
    "0": "abcdef", "1": "bc", "2": "abged", "3": "abgcd", "4": "fgbc",
    "5": "afgcd", "6": "afgedc", "7": "abc", "8": "abcdefg", "9": "abcdfg",
}
T = 8  # segment thickness

def glyph(ch):
    """Return CH rows of CW*3 bytes for one character."""
    px = [bytearray(BG * CW) for _ in range(CH)]
    def rect(x, y, w, h):
        for yy in range(y, min(y + h, CH)):
            row = px[yy]
            for xx in range(x, min(x + w, CW)):
                row[xx * 3:xx * 3 + 3] = FG
    if ch in SEG:
        s = SEG[ch]
        half = CH // 2
        if "a" in s: rect(0, 0, CW, T)
        if "b" in s: rect(CW - T, 0, T, half)
        if "c" in s: rect(CW - T, half, T, CH - half)
        if "d" in s: rect(0, CH - T, CW, T)
        if "e" in s: rect(0, half, T, CH - half)
        if "f" in s: rect(0, 0, T, half)
        if "g" in s: rect(0, half - T // 2, CW, T)
    elif ch == ":":
        rect(CW // 2 - T // 2, CH // 3 - T // 2, T, T)
        rect(CW // 2 - T // 2, 2 * CH // 3 - T // 2, T, T)
    elif ch == ".":
        rect(CW // 2 - T // 2, CH - T, T, T)
    return [bytes(r) for r in px]

GLYPHS = {c: glyph(c) for c in "0123456789:."}
ROW_BYTES = W * 3
base = bytearray(BG * (W * H))
# a moving bar row so a frozen frame is visibly different from a live one
def frame(text, tick):
    buf = bytearray(base)
    for i, ch in enumerate(text):
        g = GLYPHS[ch]
        x = X0 + i * (CW + GAP)
        for r in range(CH):
            off = (Y0 + r) * ROW_BYTES + x * 3
            buf[off:off + CW * 3] = g[r]
    bx = (tick * 6) % (W - 40)
    for r in range(H - 40, H - 20):
        off = r * ROW_BYTES + bx * 3
        buf[off:off + 40 * 3] = BAR * 40
    return bytes(buf)

out = sys.stdout.buffer
start = time.time()
n = 0
while True:
    now = time.time()
    text = time.strftime("%H:%M:%S", time.gmtime(now)) + ".%03d" % int((now % 1) * 1000)
    out.write(frame(text, n))
    out.flush()
    n += 1
    target = start + n / FPS
    d = target - time.time()
    if d > 0:
        time.sleep(d)
