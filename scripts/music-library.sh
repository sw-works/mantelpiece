#!/usr/bin/env bash
# The reel music library (F-713): fetch a few CC BY tracks from Wikimedia Commons, transcode
# them to AAC-LC in .m4a (what Vega's media pipeline plays, decision 39), and write their beat
# grids to services/crates/reels/assets/music.json, which the planner embeds.
#
# The audio lands in infra/assets/music/ (gitignored); the stack deploys it to the media
# bucket under music/. The JSON is committed, so builds and tests need neither the audio nor
# the network. Rerun after changing the track list. Needs curl, ffmpeg and uv.
set -euo pipefail
cd "$(dirname "$0")/.."
AUDIO=infra/assets/music
JSON=services/crates/reels/assets/music.json
mkdir -p "$AUDIO" "$(dirname "$JSON")"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# id | Commons file | title | artist | licence
TRACKS=(
  "carefree|Kevin_MacLeod_-_Carefree.ogg|Carefree|Kevin MacLeod|CC BY 3.0"
  "living-voyage|Kevin_MacLeod_-_Living_Voyage.ogg|Living Voyage|Kevin MacLeod|CC BY 3.0"
  "early-riser|Kevin_MacLeod_-_Early_Riser.ogg|Early Riser|Kevin MacLeod|CC BY 3.0"
)

entries=()
for row in "${TRACKS[@]}"; do
  IFS='|' read -r id file title artist licence <<<"$row"
  page="https://commons.wikimedia.org/wiki/File:$file"
  url=$(curl -fsS -A "mantelpiece-music/1.0" \
    "https://commons.wikimedia.org/w/api.php?action=query&prop=imageinfo&iiprop=url&format=json&titles=File:$file" |
    python3 -c 'import json,sys; p=next(iter(json.load(sys.stdin)["query"]["pages"].values())); print(p["imageinfo"][0]["url"])')
  curl -fsS -A "mantelpiece-music/1.0" -o "$tmp/$id.ogg" "$url"
  ffmpeg -v error -y -i "$tmp/$id.ogg" -c:a aac -b:a 192k -ar 48000 -ac 2 -movflags +faststart "$AUDIO/$id.m4a"
  # Analyse the transcoded file as it decodes (edit list applied), so the grid matches playback.
  ffmpeg -v error -y -i "$AUDIO/$id.m4a" -ac 1 -ar 22050 "$tmp/$id.wav"
  grid=$(uv run -q scripts/music/beats.py "$tmp/$id.wav" 2>/dev/null)
  entries+=("$(python3 -c '
import json, sys
grid = json.loads(sys.argv[1])
print(json.dumps({"id": sys.argv[2], "title": sys.argv[3], "artist": sys.argv[4], "licence": sys.argv[5],
                  "source": sys.argv[6], "file": "music/" + sys.argv[2] + ".m4a", **grid}))
' "$grid" "$id" "$title" "$artist" "$licence" "$page")")
  echo "$id: $(du -h "$AUDIO/$id.m4a" | cut -f1)"
done

printf '%s\n' "${entries[@]}" | python3 -c '
import json, sys
tracks = [json.loads(line) for line in sys.stdin if line.strip()]
json.dump({"tracks": tracks}, open(sys.argv[1], "w"), indent=1)
' "$JSON"
echo "wrote $JSON"
