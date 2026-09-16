#!/usr/bin/env bash
# Push a UTC wall-clock test pattern to an IVS channel for the live-latency spike
# (apps/tv-vega/spike/README.md). Frames come from clock-frames.py, paced at 30 fps, so
# each frame carries the wall time it was generated at. Needs ffmpeg with libx264.
#
#   scripts/live-spike/push-clock.sh <channel.json from `aws ivs create-channel`>
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
CHANNEL="${1:?usage: push-clock.sh <ivs create-channel output json>}"
INGEST=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['channel']['ingestEndpoint'])" "$CHANNEL")
KEY=$(python3 -c "import json,sys;print(json.load(open(sys.argv[1]))['streamKey']['value'])" "$CHANNEL")

exec python3 "$DIR/clock-frames.py" | ffmpeg -hide_banner -loglevel warning \
  -f rawvideo -pix_fmt rgb24 -s 640x360 -r 30 -i pipe:0 \
  -f lavfi -i "sine=frequency=440:sample_rate=48000" \
  -vf "scale=1280:720:flags=neighbor" \
  -c:v libx264 -preset veryfast -tune zerolatency -profile:v main -pix_fmt yuv420p \
  -b:v 2500k -maxrate 2500k -bufsize 2500k -g 60 -keyint_min 60 -bf 0 -sc_threshold 0 \
  -c:a aac -b:a 96k -ar 48000 -ac 2 -shortest \
  -f flv "rtmps://$INGEST:443/app/$KEY"
