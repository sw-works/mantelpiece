#!/usr/bin/env python3
"""Download a public, openly licensed travel album as test photos.

Default: "Photographs by Sergei Gussev, Iceland" on Wikimedia Commons: one photographer,
six consecutive days in June 2015, GPS in nearly every frame, CC BY 2.0. Originals keep
their EXIF, which is what the ingest pipeline reads for days, routes and place names.

    scripts/fetch-test-album.py                 # 10 photos per day into testbed/iceland-2015
    scripts/fetch-test-album.py --per-day 20
    scripts/fetch-test-album.py --category "Category:..." --out testbed/other

Writes ATTRIBUTION.md next to the photos: the licence requires credit wherever they are
shown, the demo video included. Standard library only; polite to the Commons API.
"""

import argparse
import collections
import json
import pathlib
import re
import time
import urllib.error
import urllib.parse
import urllib.request

API = "https://commons.wikimedia.org/w/api.php"
AGENT = {"User-Agent": "memorylane-testbed/0.1 (https://github.com; hackathon test data)"}
DEFAULT = "Category:Photographs by Sergei Gussev, Iceland"


def get(url: str, tries: int = 6) -> bytes:
    for attempt in range(tries):
        try:
            time.sleep(1.0)
            with urllib.request.urlopen(urllib.request.Request(url, headers=AGENT), timeout=120) as r:
                return r.read()
        except urllib.error.HTTPError as e:
            if e.code != 429:
                raise
            time.sleep(int(e.headers.get("Retry-After") or 20) * (attempt + 1))
    raise SystemExit("rate limited by Wikimedia Commons; try again later")


def files(category: str) -> list[dict]:
    out, cont = [], {}
    while True:
        params = dict(
            action="query", format="json", generator="categorymembers", gcmtitle=category,
            gcmtype="file", gcmlimit=50, prop="imageinfo",
            iiprop="url|size|mime|metadata|extmetadata", **cont,
        )
        page = json.loads(get(f"{API}?{urllib.parse.urlencode(params)}"))
        for p in page.get("query", {}).get("pages", {}).values():
            info = (p.get("imageinfo") or [{}])[0]
            meta = {m["name"]: m["value"] for m in info.get("metadata") or [] if isinstance(m.get("value"), (str, int, float))}
            ext = info.get("extmetadata", {})
            out.append({
                "title": p["title"], "url": info.get("url"), "page": info.get("descriptionurl"),
                "bytes": info.get("size", 0), "mime": info.get("mime"),
                "taken": meta.get("DateTimeOriginal"), "gps": "GPSLatitude" in meta,
                "artist": re.sub(r"<[^>]+>", "", ext.get("Artist", {}).get("value", "")).strip(),
                "licence": ext.get("LicenseShortName", {}).get("value", ""),
                "licence_url": ext.get("LicenseUrl", {}).get("value", ""),
            })
        if "continue" not in page:
            return out
        cont = page["continue"]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--category", default=DEFAULT)
    ap.add_argument("--out", default="testbed/iceland-2015")
    ap.add_argument("--per-day", type=int, default=10)
    ap.add_argument("--max-mb", type=float, default=12.0, help="skip originals larger than this")
    args = ap.parse_args()

    usable = [f for f in files(args.category) if f["mime"] == "image/jpeg" and f["taken"] and f["gps"] and f["bytes"] <= args.max_mb * 1e6]
    days = collections.defaultdict(list)
    for f in sorted(usable, key=lambda f: f["taken"]):
        days[f["taken"][:10]].append(f)
    chosen = []
    for day, shots in sorted(days.items()):
        step = max(1, len(shots) // args.per_day)  # spread across the day, not the first ten
        chosen += shots[::step][: args.per_day]

    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    for i, f in enumerate(chosen, 1):
        name = f"{f['taken'][:10].replace(':', '-')}_{i:03d}.jpg"
        target = out / name
        if not target.exists():
            target.write_bytes(get(f["url"]))
        f["file"] = name
        print(f"{i}/{len(chosen)} {name} {f['bytes'] / 1e6:.1f} MB")

    lines = [f"# Test album: {args.category}", "", f"Source: https://commons.wikimedia.org/wiki/{urllib.parse.quote(args.category.replace(' ', '_'))}", "",
             "Credit these photos wherever they are shown.", "", "| File | Photographer | Licence | Source |", "|---|---|---|---|"]
    lines += [f"| {f['file']} | {f['artist']} | [{f['licence']}]({f['licence_url']}) | [{f['title']}]({f['page']}) |" for f in chosen]
    (out / "ATTRIBUTION.md").write_text("\n".join(lines) + "\n")
    per_day = collections.Counter(f["taken"][:10] for f in chosen)
    print(f"{len(chosen)} photos over {len(per_day)} days in {out}: {dict(sorted(per_day.items()))}")


if __name__ == "__main__":
    main()
