#!/usr/bin/env bash
# Screenshot of the Vega Virtual Device, host-rendered. Neither gwsi-tool-screenshooter
# (hangs on the Virtual Device) nor a QEMU QMP screendump (black: rendering goes through
# host GL) works; the Virtual Device is an Android-emulator fork and its console on port
# 5554 does. Verified on SDK 0.24.9914, 2026-09-09.
#
#   scripts/vvd-screenshot.sh docs/screens/home.png
set -euo pipefail
OUT="${1:?usage: vvd-screenshot.sh <out.png>}"
PORT="${VVD_CONSOLE_PORT:-5554}"
python3 - "$OUT" "$PORT" <<'PY'
import glob, os, shutil, socket, sys, tempfile, time
out, port = sys.argv[1], int(sys.argv[2])
tmp = tempfile.mkdtemp()
s = socket.create_connection(("127.0.0.1", port), timeout=10)
def rx(t=3):
    s.settimeout(t); buf = b""
    try:
        while True:
            c = s.recv(65536)
            if not c: break
            buf += c
            if buf.rstrip().endswith((b"OK", b"KO")) or b"KO:" in buf: break
    except socket.timeout: pass
    return buf.decode(errors="replace")
rx()
tok = os.path.expanduser("~/.emulator_console_auth_token")
if os.path.exists(tok):
    s.sendall(f"auth {open(tok).read().strip()}\n".encode()); rx()
s.sendall(f"screenrecord screenshot {tmp}\n".encode())
if "KO" in rx(10): sys.exit("screenshot refused by the emulator console")
for _ in range(20):
    files = glob.glob(os.path.join(tmp, "*.png"))
    if files: break
    time.sleep(0.5)
else: sys.exit("no screenshot written")
os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
shutil.move(files[0], out); shutil.rmtree(tmp, ignore_errors=True)
print(f"saved {out}")
PY
