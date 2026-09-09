<!--
Sync Impact Report
- Version change: (template, unversioned) → 1.0.0
- Modified principles: none (initial ratification)
- Added sections: Core Principles (I–VI), Platform Constraints, Development Workflow, Governance
- Removed sections: none
- Templates: plan/spec/tasks templates read this file at runtime; no template edits needed
- Follow-up TODOs: none
-->

# MemoryLane Constitution

## Core Principles

### I. Zero Infrastructure for the User

The user installs one Fire TV app, scans one QR with a phone, and is done. No home server,
no network configuration, no tunnel, no personal AWS account, no app to install on the phone.
Every feature MUST work with only a Fire TV, a phone browser, and the hosted backend. A
feature that needs a hub, a sideload, or a native phone app is out of scope until the
constitution is amended. Rationale: the hub design was dropped on 2026-09-08 because it cost
the user a server, a tunnel, and their own deployment; that cost must not creep back.

### II. Vega-First TV App

The Fire TV app targets Vega OS first and MUST build from React Native core plus `fetch`,
timers, images over HTTPS, and system-deployed `@amazon-devices/*` ports. No native module
is bundled by this repo. Platform differences live in `.kepler.ts` files beside the default
implementation. Fire OS from the same `packages/tv-ui` code is a stretch goal, never a reason
to add Android-only dependencies. The React Native version of the Vega shell is set by the
Vega SDK (0.72, or 0.83 where available), not by the newest upstream release. Rationale: all
new Fire TV Sticks ship Vega, the hackathon demo runs on a Vega device or Virtual Device, and
every bundled native module is a build the spike cannot verify.

### III. Confirm Before Irreversible (NON-NEGOTIABLE)

No agent, workflow, or Alexa+ tool applies an irreversible change without a confirmation
token minted by the TV's Review Changes modal and checked by Cedar policy at the AgentCore
Gateway. Irreversible today: `apply_dedup`, `apply_cleanup`, `delete_media`, `share_album`,
`remove_contributor`, and any co-editor removal of another contributor's photo. Agents
MUST propose, never self-confirm. Before-and-after counts and sample thumbnails MUST be shown
before the token is minted. Rationale: family photo archives cannot be regenerated; a single
unconfirmed deletion breaks the trust the whole product depends on.

### IV. Narrative From Structured Data Only

Every model-written story, caption, or answer MUST be generated from metadata the pipeline
produced (timestamps, GPS, labels, contributors, trip and day structure) and checked against
it before it is saved. A story naming a place or date absent from the metadata is withheld,
not shown. Every generated text carries its model id and generation time and can be
regenerated on demand. Rationale: an invented place in a family memory is worse than no
story; the fact check is cheap and deterministic.

### V. Deterministic Before Expensive

Ingest runs checksum, EXIF, perceptual hash, sharpness, exposure, and burst grouping in
deterministic Rust before any model call. Only burst keepers reach Rekognition or Bedrock;
the rest of a burst inherits. Retrieval for Ask is vector search plus filters with a 300 ms
warm budget inside a trip; a second model call is not allowed in that path. Rationale: cost
scales with the burst ratio (3 to 5x on phone archives), and D-pad and voice interfaces
cannot wait on a model.

### VI. Tests Run Without AWS

`make test` (cargo test, vitest and jest, pytest) MUST pass on a laptop and in CI with no
AWS credentials. Storage, blob, queue, and model backends are traits or interfaces with
in-memory implementations; the AWS implementations are compile-checked until a stack is
deployed. No `@amazon-devices/*` package enters `package.json` on `main` until CI can
install it. Rationale: the hackathon team has one deployed account; every contributor and
every CI run must still be able to prove the code works.

## Platform Constraints

- Backend: Rust on Lambda (arm64 via cargo-lambda) behind API Gateway; Step Functions for
  ingest; DynamoDB single table keyed by household; S3 for originals and renditions;
  EventBridge for workflow triggers; one CDK stack deploys everything (F-606).
- Agents: Strands on Bedrock AgentCore Runtime with Memory; tools only through AgentCore
  Gateway with Lambda targets that are the same API Lambda; Cedar policy on irreversible
  tools. Alexa+ reaches the same Gateway endpoint over MCP (spec 2025-11-25, Streamable
  HTTP, OAuth 2.1 with PKCE).
- Phone: a PWA (`apps/web`), no install; photos enter through the browser file picker only,
  because Amazon Photos exposes no public API. Uploads are resumable S3 multipart with a
  streaming SHA-256 for exact-duplicate rejection.
- Voice: never captured by the TV app (Vega exposes no microphone API). Voice reaches the
  same tools through Alexa+ or the phone page's browser speech recognition.
- Privacy: originals live in S3 under the user's identity, encrypted at rest and in transit;
  Bedrock and Rekognition do not train on customer data; per-household isolation across
  storage, index, memory, and tools (F-602); account deletion removes originals, renditions,
  embeddings, memory, and face vectors (F-604); face matching is opt-in per household.
- Sharing roles: owner, co-editor, contributor, member household (F-162 to F-165). Guests and
  the read-only web view never edit.

## Development Workflow

- Specs are the source of truth: `specs/architecture.md` (requirements F-101 to F-710,
  workflows W1 to W6, open questions), `specs/hackathon.md` (rules, timeline, demo), and
  dated decision logs (`specs/decisions-YYYY-MM-DD.md`). A feature spec under Spec Kit MUST
  cite the F-numbers it implements and MUST NOT contradict a decision log without amending it.
- `make lint` (rustfmt, clippy with `-D warnings`, eslint, tsc, ruff) and `make test` gate
  every pull request. CI runs both on every push to `main` and every pull request.
- Conventional commit messages (`feat`, `fix`, `docs`, `chore`, `refactor`, `test`), one
  concern per commit.
- Claims from documentation are marked *unverified* until checked on a Vega Virtual Device
  or a Fire TV Stick; the decision log records which.
- Diagrams in `docs/diagrams` are drawn from code as built; planned changes go in a dated
  planned-delta diagram, never into the as-built set.
- A friction log and feature requests are kept from week one for the hackathon submission.

## Governance

This constitution supersedes README, CLAUDE.md, and per-directory conventions where they
conflict. Amendments are made by editing this file with a version bump and a Sync Impact
Report, recorded in a dated decision log under `specs/`, and merged through a pull request.
Versioning follows semantic versioning: MAJOR for removing or redefining a principle, MINOR
for adding a principle or materially expanding a section, PATCH for wording. Every pull
request review MUST check the change against Principles I to VI; any exception MUST be named
in the pull request and, if it survives, encoded here as an amendment. Runtime guidance for
agents lives in `CLAUDE.md` and defers to this file.

**Version**: 1.0.0 | **Ratified**: 2026-09-08 | **Last Amended**: 2026-09-09
