#!/usr/bin/env bash
# Fetch a static arm64 ffmpeg into infra/assets/ffmpeg-layer/bin so the CDK stack packages it
# as the poster Lambda's layer (research R5). Any static build works; the John Van Sickle
# release is GPL and about 80 MB unpacked, well under the 250 MB layer limit. A trimmed
# h264/hevc/aac-only build would be smaller but needs a custom compile. Not run in CI; the
# stack falls back to a placeholder layer when the binary is absent.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=infra/assets/ffmpeg-layer/bin
URL="${FFMPEG_STATIC_URL:-https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-arm64-static.tar.xz}"
mkdir -p "$OUT"
tmp=$(mktemp -d)
curl -fL "$URL" -o "$tmp/ffmpeg.tar.xz"
tar -xJf "$tmp/ffmpeg.tar.xz" -C "$tmp"
cp "$(find "$tmp" -type f -name ffmpeg | head -1)" "$OUT/ffmpeg"
chmod +x "$OUT/ffmpeg"
rm -rf "$tmp"
du -sh "$OUT"
echo "ffmpeg layer ready at $OUT (deploy with make deploy)"
