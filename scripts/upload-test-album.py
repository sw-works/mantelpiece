#!/usr/bin/env python3
"""Pair with the TV like a phone and upload a folder of photos.

    scripts/fetch-test-album.py                       # once: the Iceland test album
    scripts/upload-test-album.py --code 4KB58PJP      # the code under the QR on the TV

The script asks to connect under --name; press Allow on the TV and the upload starts. The
API base comes from apps/tv-vega/app.json (written by `make up`) unless --api is given.
Standard library only.
"""

import argparse
import glob
import hashlib
import json
import pathlib
import ssl
import time
import urllib.error
import urllib.request
import uuid

ROOT = pathlib.Path(__file__).resolve().parent.parent
# python.org builds on macOS ship without CA certificates; the system bundle works.
CTX = ssl.create_default_context(cafile="/etc/ssl/cert.pem") if pathlib.Path("/etc/ssl/cert.pem").exists() else None


def call(api, method, path, body=None, bearer=None, timeout=60):
    req = urllib.request.Request(api + path, method=method, data=json.dumps(body).encode() if body is not None else None)
    req.add_header("content-type", "application/json")
    if bearer:
        req.add_header("authorization", f"Bearer {bearer}")
    try:
        with urllib.request.urlopen(req, timeout=timeout, context=CTX) as r:
            return json.loads(r.read() or b"null")
    except urllib.error.HTTPError as e:
        raise SystemExit(f"{method} {path}: {e.code} {e.read().decode()[:300]}") from e


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--code", required=True, help="pairing code shown on the TV")
    ap.add_argument("--photos", default=str(ROOT / "testbed" / "iceland-2015"))
    ap.add_argument("--name", default="Test album")
    ap.add_argument("--api", default=json.loads((ROOT / "apps/tv-vega/app.json").read_text()).get("apiBase", ""))
    args = ap.parse_args()
    if not args.api:
        raise SystemExit("no API base: run `make up` first or pass --api")
    files = sorted(f for ext in ("jpg", "jpeg", "JPG", "JPEG", "heic", "HEIC") for f in glob.glob(f"{args.photos}/*.{ext}"))
    if not files:
        raise SystemExit(f"no photos in {args.photos}: run scripts/fetch-test-album.py first")

    code = args.code.replace(" ", "").upper()
    tv = call(args.api, "GET", f"/pair/code/{code}")["tv_name"]
    asked = call(args.api, "POST", "/pair/requests", {"code": code, "name": args.name})
    print(f'Asked to connect to "{tv}" as "{args.name}". Press Allow on the TV.')
    while True:
        status = call(args.api, "GET", "/pair/status", bearer=asked["token"], timeout=90)
        if status["state"] == "approved":
            break
        if status["state"] == "denied":
            raise SystemExit("Not allowed on the TV.")
        time.sleep(1)
    phone, batch = status["token"], str(uuid.uuid4())
    print(f"Connected. Uploading {len(files)} photos.")
    for i, f in enumerate(files, 1):
        data = pathlib.Path(f).read_bytes()
        try:
            init = call(args.api, "POST", "/uploads/init", {"filename": pathlib.Path(f).name, "bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(), "content_type": "image/jpeg", "batch_id": batch}, phone)
        except SystemExit as e:
            print(f"{i}/{len(files)} skipped: {e}")
            continue
        parts = []
        for n, url in enumerate(init["part_urls"], 1):
            chunk = data[(n - 1) * init["part_size"] : n * init["part_size"]]
            # A dropped connection on one part should not end a 48-photo run: try three times.
            for attempt in range(3):
                try:
                    with urllib.request.urlopen(urllib.request.Request(url, method="PUT", data=chunk), timeout=300, context=CTX) as r:
                        parts.append({"part_number": n, "etag": r.headers["ETag"]})
                    break
                except (urllib.error.URLError, ConnectionError, TimeoutError):
                    if attempt == 2:
                        raise
                    time.sleep(2 + attempt * 3)
        call(args.api, "POST", "/uploads/complete", {"media_id": init["media_id"], "upload_id": init["upload_id"], "parts": parts, "batch_done": i, "batch_total": len(files)}, phone)
        print(f"{i}/{len(files)} {pathlib.Path(f).name}")
    print("Done. The TV shows the trip once it is organised, usually within a minute or two.")


if __name__ == "__main__":
    main()
