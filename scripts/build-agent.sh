#!/usr/bin/env bash
# Package the Concierge agent for AgentCore Runtime's direct code deployment: the agent's
# own code plus its runtime dependencies as Linux arm64 wheels, in one directory the CDK
# stack zips and uploads. Needs uv. Output: agents/build/runtime (gitignored).
set -euo pipefail
cd "$(dirname "$0")/../agents"
OUT="build/runtime"
PY="${AGENT_PYTHON:-3.13}"
rm -rf "$OUT" && mkdir -p "$OUT"
uv pip install --quiet \
  --target "$OUT" \
  --python-platform aarch64-manylinux2014 \
  --python-version "$PY" \
  --only-binary=:all: \
  "bedrock-agentcore>=0.1" "strands-agents>=1.0"
cp -R src/memorylane_agents "$OUT/"
# The Runtime starts this file; everything else is importable next to it.
cat > "$OUT/main.py" <<'PYEOF'
from memorylane_agents.runtime import main

if __name__ == "__main__":
    main()
PYEOF
find "$OUT" -name "__pycache__" -type d -prune -exec rm -rf {} +
du -sh "$OUT" | awk '{print "agent package: " $1}'
