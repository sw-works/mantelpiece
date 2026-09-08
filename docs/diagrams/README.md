# Diagrams

SVG source, built by hand on explicit coordinates so diffs are meaningful and the visual
encoding holds across the set. Grey is deterministic code, blue is a model call, amber is a
human decision, red is a security service, dashed is a store or queue, dotted is an external
system.

## Workflow diagrams

Drawn from the code as built on 2026-09-08. Dense by design; for checking a claim against source.

| # | Draws | Source |
|---|-------|--------|
| 01 | Topology: clients, API, S3, ingest state machines, DynamoDB, EventBridge, AgentCore, Alexa+, and the confirmation-token gate | `services/lambdas/api`, `services/lambdas/ingest`, `infra/lib/memorylane-stack.ts`, `agents/src/memorylane_agents` |

## Regenerate

```sh
node docs/diagrams/src/build.mjs        # all
node docs/diagrams/src/build.mjs 01     # by filename fragment
```

SVG needs nothing. PNG needs playwright (`npm i -g playwright && npx playwright install chromium`).
