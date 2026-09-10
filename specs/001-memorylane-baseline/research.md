# Research: MemoryLane Baseline

Each item resolves an unknown from the plan's Technical Context or an open question in
`specs/architecture.md` section 9. "Verified" says how far the answer has been checked:
docs (Amazon documentation, via Builder Tools search or the web), code (this repo), device
(Vega Virtual Device or Fire TV Stick). Nothing below has been checked on a device yet.

## R1. React Native version for the Vega shell

- Decision: `apps/tv-vega` pins the React Native version the installed Vega SDK provides:
  0.83 if SDK 0.24's early access is reachable from this account, otherwise 0.72. Shared
  code in `packages/tv-ui` stays on React Native core APIs only so it compiles under either.
- Rationale: React Native for Vega supports 0.72 and, since SDK 0.24 (August 2026), 0.83
  with React 19.2 and Hermes v1. Our `apps/tv` at 0.87 builds for neither. The split mirrors
  the AmazonAppDev multi-TV sample's `packages/shared-ui` plus thin shells.
- Alternatives: downgrade `apps/tv` in place to 0.72 (loses React 19 hooks style and the
  jest 30 toolchain for everyone); wait for a 0.87 port (no date).
- Verified: docs (vega-rn-overview 0.24, SDK 0.24 release notes, Builder Tools alpha note).

## R2. Persisting the TV session without a bundled native module

- Decision: use `@amazon-devices/react-native-async-storage__async-storage`, aliased as
  `@react-native-async-storage/async-storage` through a package.json `npm:` alias and
  pnpm override in `apps/tv-vega` only. `packages/tv-ui` keeps the `SessionStore` interface
  and its in-memory implementation for tests.
- Rationale: the library is system-deployed (native side in the OS, autolinked at runtime),
  so the constitution's "no bundled native module" holds. Same for
  `@amazon-devices/react-native-qrcode-svg`, kept optional since the View-based QR works.
- Alternatives: keep the in-memory store (TV re-pairs every launch, fails SC-001 on
  restart); MMKV port (marked partial support).
- Verified: docs (vega-api react-native-async-storage, supported libraries list).

## R3. Voice input

- Decision: the TV never captures audio. Voice reaches the same tools two ways: Alexa+ via
  the MCP add-on, and the phone's Ask page using the browser's Web Speech API. The TV posts
  a now-showing presence record so "that photo" resolves server-side.
- Rationale: no app-level microphone API exists on Vega; Voice Search is a system overlay
  that blurs the app. Alexa+ add-ons receive no on-screen context, so the app must publish
  it.
- Alternatives: Fire OS SpeechRecognizer (Android only, against Vega-first); a W3C media
  port for audio capture (none documented).
- Verified: docs (Builder Tools search, Alexa+ MCP toolkit overview).

## R4. AWS region

- Decision: us-east-1 first. Fall back to us-west-2 only if AgentCore Policy or Bedrock
  model access is missing there at deploy time; the CDK stack takes the region from the
  environment and nothing is hard-coded.
- Rationale: widest Bedrock model and AgentCore coverage; Amazon Location and Rekognition
  are available in both.
- Alternatives: us-west-2 first (equal for our services, slightly less model coverage).
- Verified: docs only; confirm at first deploy (week 1 spike).

## R5. HEIC decode and video poster frames on Lambda

- Decision: statically link libheif and turbojpeg into the ingest Lambda via cargo-lambda
  on arm64 (already in `crates/ingest/decode.rs`); video poster frames from a separate
  Lambda with an ffmpeg layer trimmed to the decoders needed (h264, hevc, aac) to stay
  under 250 MB unzipped. No transcoding of originals.
- Rationale: iOS uploads HEIC and MOV by default; the TV receives JPEG only (F-105).
- Alternatives: MediaConvert for posters (slower, costs per job, overkill for a frame);
  client-side conversion on the phone (drains battery, blocks upload).
- Verified: code (decode.rs compiles with the features); the arm64 static build and layer
  size are unverified until the first `make build-lambdas` on Amazon Linux 2023.

## R6. Alexa+ MCP endpoint requirements

- Decision: expose the AgentCore Gateway MCP endpoint behind OAuth 2.1 with PKCE, a
  protected resource metadata document at `/.well-known/oauth-authorization-server`, and
  MCP spec 2025-11-25 over Streamable HTTP. Every tool answers in under 500 ms by reading
  precomputed data: descriptions, landmarks and captions are produced at enrich time, so
  `describe_photo` is a lookup with a live model call only as fallback.
- Rationale: Alexa+ quickstart requires the spec version, transport, auth shape and a
  500 ms round trip. Cognito is already the identity provider; its OAuth server issues the
  tokens and the resource server declares scopes.
- Alternatives: a separate self-hosted MCP server in front of the Gateway (second surface
  to secure); simulated Alexa+ web chat only (allowed by the rules, kept as fallback).
- Verified: docs (Alexa+ MCP toolkit quickstart). Gateway's exact MCP spec version is open
  question 3 and is checked at deploy.

## R7. Vector search store

- Decision: per-trip embedding cache in DynamoDB loaded into the query Lambda's memory on
  first hit for within-trip queries; S3 Vectors for cross-trip queries when available in
  the deploy region, else a DynamoDB scan over the household's keeper embeddings (10k
  vectors, acceptable for the hackathon).
- Rationale: F-303 needs 300 ms warm inside a trip; S3 Vectors latency is sub-second to
  low-second, too slow for D-pad browsing but fine across trips.
- Alternatives: OpenSearch Serverless (cost and cold start), Aurora pgvector (a database to
  run).
- Verified: docs; S3 Vectors regional availability is open question 4.

## R8. Photo descriptions and "what's that statue"

- Decision: at enrich time each keeper gets a short description and landmark guess from a
  Bedrock multimodal model given the rendition, EXIF GPS, Rekognition labels and detected
  text. Stored with model id and time. `describe_photo(media_id, question)` answers from
  that record plus trip and day context; a live model call happens only when the stored
  description cannot answer, and its output passes the same place and date check as the
  narrative before it is shown.
- Rationale: Rekognition labels say "Statue", never which one; GPS plus image identifies
  landmarks well. Precomputing meets the Alexa+ latency budget and constitution V.
- Alternatives: live vision call on every question (seconds, fails the budget); labels only
  (cannot name landmarks).
- Verified: design only; model choice (Nova Pro versus Claude) decided by cost at deploy.

## R9. People (faces) with consent

- Decision: opt-in per household. On enable, enrich indexes keeper faces into a
  per-household Rekognition collection; clusters come from `SearchFaces` matches; names are
  set from the phone page. Disable or account deletion deletes the collection. Pets are
  label search only.
- Rationale: Rekognition stores face vectors, not images; per-household collections keep
  isolation (F-602) and make deletion one call (F-604).
- Alternatives: on-device face grouping (no compute on the TV), a third-party face API (no).
- Verified: docs; consent copy is in the mockups and the privacy position.

## R10. Audio playback for narration on Vega

- Decision: implement F-709 behind a feature flag. The Vega shell tries the system-deployed
  W3C media port for audio playback of Polly output; if the port is not system-deployed or
  the spike fails, narration ships as on-screen caption text only and the flag stays off.
- Rationale: the reference sample used `@amazon-devices/react-native-w3cmedia` as a
  dependency, not a core API, and the ported-library table does not list it as
  system-deployed. Unknown until the Virtual Device spike.
- Alternatives: play narration on the phone (odd in a living room); skip narration.
- Verified: docs partial; open question 10, device check in week 1.

## Verification schedule

| Item | Where | When |
|------|-------|------|
| R1 RN version, R2 AsyncStorage, R10 audio | Vega Virtual Device | week 1 spike |
| R4 region, R6 Gateway spec version, R7 S3 Vectors | first `make deploy` | week 1 |
| R5 static libheif and ffmpeg layer | `make build-lambdas` on AL2023 | week 1 |
| R3 phone speech, R8 descriptions, R9 faces | deployed stack with sample archive | weeks 2 to 4 |
