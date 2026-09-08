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
stack is deployed. Vega packaging of `apps/tv` waits on the Kepler CLI spike (week 1).

## License

MIT, see [LICENSE](LICENSE).
