#!/usr/bin/env bash
# Point the Vega shell at the deployed API: writes `apiBase` in apps/tv-vega/app.json from
# the stack's ApiUrl output. Run before `make tv-vvd-install` or a .vpkg build. Pass a URL
# to skip CloudFormation (e.g. a local dev server).
set -euo pipefail
cd "$(dirname "$0")/.."
STACK="${STACK_NAME:-Mantelpiece}"
API="${1:-$(aws cloudformation describe-stacks --stack-name "$STACK" --query "Stacks[0].Outputs[?OutputKey=='ApiUrl'].OutputValue" --output text)}"
node -e '
const fs = require("fs");
const p = "apps/tv-vega/app.json";
const j = JSON.parse(fs.readFileSync(p, "utf8"));
j.apiBase = process.argv[1].replace(/\/+$/, "");
fs.writeFileSync(p, JSON.stringify(j, null, 2) + "\n");
console.log("apps/tv-vega/app.json apiBase =", j.apiBase);
' "$API"
