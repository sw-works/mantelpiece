# MemoryLane

Turn a family trip into a shared album that builds itself, on the living-room TV.

MemoryLane is a Fire TV app plus a hosted backend. Install the app, scan the QR on the TV,
pick photos on your phone. Everyone on the trip scans the same QR and adds theirs. The
backend groups bursts, scores highlights, reconstructs the route, writes a day-by-day story,
and an agent keeps the archive tidy. Ask about any trip from the TV or from Alexa+.

Entry for Build, Ship, Shape: Amazon Developer Hackathon 2026.
Tracks: Fire TV (primary), Alexa+ (secondary). Mini-challenges: AWS Builder, Open Source.

## Documents

- [specs/architecture.md](specs/architecture.md): architecture, required features, workflows, open questions
- [specs/hackathon.md](specs/hackathon.md): entry strategy, rules, timeline, demo plan, judge access
- [specs/decisions-2026-09-09.md](specs/decisions-2026-09-09.md): Fire TV research log: sample repo, Vega RN pin, Virtual Device, Builder Tools, voice
- [docs/diagrams/](docs/diagrams/): architecture diagrams as SVG source

![Topology](docs/diagrams/01-topology.svg)

## Layout

```
apps/tv/         React Native (TypeScript) Fire TV app: fetch-only, no native modules
apps/web/        Phone pages: link (approve the TV code) and upload (join by code, resumable uploads)
services/        Rust workspace
  crates/core    domain types, device-code and trip-code flows, confirmation tokens
  crates/ingest  checksum, EXIF, pHash, quality signals, burst grouping, renditions
  crates/trips   event and trip clustering, clock alignment, routes, naming, highlights
  crates/store   Storage and Blobs traits: DynamoDB, S3, in-memory
  lambdas/api    backend API Lambda behind API Gateway
  lambdas/ingest Step Functions tasks: signal, enrich, organize
agents/          Concierge workflows W1 to W6 for AgentCore Runtime (Python, uv)
infra/           CDK stack (TypeScript)
```

## Build and test

```
make test        # cargo test, vitest/jest, pytest
make lint        # rustfmt + clippy, eslint + tsc, ruff
make build-lambdas   # cargo-lambda, arm64, into services/target/lambda
make deploy      # cdk deploy (needs AWS credentials and Login with Amazon client id/secret)
```

Everything under `make test` runs without AWS. The DynamoDB, S3, Step Functions,
Rekognition, EventBridge and AgentCore Gateway backends are compile-checked only until the
stack is deployed.

### Fire TV app

Requires the [Amazon Vega SDK](https://developer.amazon.com/docs/vega/0.21/install-vega-sdk.html)
with `KEPLER_SDK_HOME` set, and `@amazon-devices/kepler-cli-platform` installed from the
SDK's registry (not on public npm, so it is not in `package.json`). The Vega SDK pins its
own React Native version; if it differs from the one in `apps/tv/package.json`, the app
moves to a `packages/tv-ui` plus per-platform app layout. Not yet verified: the packaging
below waits on the week 1 spike.

```
cd apps/tv
pnpm start                 # Metro
pnpm build:vega:debug      # react-native build-kepler, installs on a Vega Virtual Device
pnpm build:vega            # release .vpkg
```

`apps/tv/manifest.toml` declares the Vega package: one interactive component and network
access only. Platform-specific code goes in `.kepler.ts` files next to the default
implementation (see `src/hooks/useRemoteKeys.kepler.ts`); Metro picks them on Vega and
falls back to the plain file on Fire OS. Screens keep text inside the 1080p title-safe
margin defined in `src/theme.ts`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License

MIT, see [LICENSE](LICENSE).
