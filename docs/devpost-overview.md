# Devpost project overview

Submission: https://devpost.com/submit-to/30992-build-ship-shape-amazon-developer-hackathon/manage/submissions/1175821/project-overview

## Project name

MemoryLane

## Tagline

One QR on the TV turns a family trip into a shared album that builds itself.

## Track

Fire TV (primary). Alexa+ (secondary submission, same agent tools through an MCP add-on).
Mini-challenges: AWS Builder, Open Source.

## About the project

### What it does

Install MemoryLane on a Fire TV and it shows a code and a QR. Scan it with your phone,
sign in with your Amazon account, and pick photos from your camera roll. Everyone on the
trip scans the same QR from their own phone and their photos land in the same trip,
credited to them. The TV shows uploads arriving live. The same code works on another Fire TV:
grandma opens "Join a trip", types it, and the trip appears on her screen the evening it
happens, with everyone's photos and her reactions flowing back to yours.

The backend groups burst shots and picks the keeper, scores every photo, aligns the clocks
of different phones from overlapping GPS, splits the trip into days, draws the route, and
writes a day-by-day story that names who took what. Ask the TV or Alexa+ for "beach days
with the kids" and a playlist plays. Nothing irreversible happens without a confirmation
on the remote: an agent can propose removing duplicates, but only the Review Changes
modal on the TV can mint the token that lets the tool run.

### How we built it

- Fire TV app in React Native for Vega, fetch-only, no native modules, one codebase for
  Vega OS and Fire OS.
- Phone uploader as a PWA: no install, resumable multipart uploads straight to S3,
  streaming SHA-256 for exact-duplicate rejection before any processing. The phone
  remembers the trip, and on Android the page is a share-sheet target, so after the first
  scan photos go from the camera roll to the TV without opening anything.
- Backend in Rust on Lambda behind API Gateway: device-code sign-in through Cognito
  federated to Login with Amazon, trip codes, presigned uploads, a long-poll event feed.
- Ingest as Step Functions: a per-object signal task (EXIF, perceptual hash, sharpness,
  renditions) and a per-household organize task (bursts, keepers, clock alignment, trips,
  routes, highlights). Only keepers reach Rekognition; the rest of a burst inherits.
- Agents on Bedrock AgentCore: a Strands Concierge with Memory, six workflows (Trip
  Curator, Ask Your Archive, Household Patterns, Dedup Steward, Share, Trip Merge), tools
  through AgentCore Gateway with Lambda targets, Cedar policy on irreversible tools.
- Phone pages for reactions, hold-to-talk questions and the People consent switch; a second
  Fire TV joins a trip by code and gets its own feed, pins and hides.
- Expiring share links with an optional email, an ambient slideshow after idle, story
  captions with narration behind a flag, nightly On this day and Then and now.
- Every agent run and tool call is a span tagged with the household; every sign-in,
  share, deletion and gated tool leaves an audit entry with the confirmation it carried.
- One CDK stack. Everything under `make test` runs without AWS.

### Challenges

- Amazon Photos has no public API, so photos come from the phone's camera roll through the
  browser file picker. This is the biggest ecosystem gap for memory apps on Fire TV and
  Alexa+; the friction log names an Amazon Photos MCP tool and an LWA `photos:read` scope.
- Cognito has no OAuth device grant, so sign-in on the TV is a small custom device-code
  flow: the TV shows a code, the phone approves it, the TV polls.
- A trip narrative from a model must never invent a place or date. The story is generated
  from structured metadata only and checked against it before it is saved; a story that
  fails the check is withheld.

### What's next

Verify on the Virtual Device and a Fire TV Stick once the Vega SDK is installed, deploy
the stack and time the 15-minute organise target on a real archive, register the Alexa+
add-on when the preview opens (the OAuth metadata, PKCE client and scope are already
served), and a home node that syncs metadata for households that want originals to stay
local. What stayed unverified on 2026-09-09 is listed in the README and
`docs/friction-log.md`.

## Built with

Rust, React Native, TypeScript, AWS Lambda, API Gateway, Step Functions, DynamoDB, S3,
EventBridge, Cognito, Login with Amazon, Rekognition, Amazon Bedrock, Bedrock AgentCore
(Gateway, Runtime, Memory, Policy), Strands Agents, AWS CDK, Vega OS.

## Links

- Repository: https://github.com/hanyuxiao/memory-lane (public before submission)
- Architecture: `specs/architecture.md`, diagrams in `docs/diagrams/`
- Friction log: `docs/friction-log.md`; Alexa+ add-on: `docs/alexa-plus.md`
