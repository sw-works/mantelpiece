# MemoryLane: Hackathon Plan

Status: Draft for hackathon execution
Target: Build, Ship, Shape: Amazon Developer Hackathon (Devpost)
Submission deadline: 2026-10-23 12:00 PT
Judging: 2026-11-09 to 2026-11-20
Companion: `architecture.md` (design, requirements, workflows, open questions)
Created: 2026-09-08

## 1. Entry Strategy

- Primary track: **Fire TV** (Vega OS + Fire OS, one React Native codebase)
- Secondary track: **Alexa+** (same AgentCore Gateway MCP endpoint, separate submission)
- Mini-challenge: **AWS Builder** (Bedrock + AgentCore Gateway, Runtime, Memory, Policy;
  service list in `architecture.md` section 7)
- Mini-challenge: **Open Source** (public repo with MIT license; new features land during the
  window)

Alexa+ fallback: if preview access is not granted, the rules accept a simulated web
experience. Ship a web chat backed by the Runtime agent through the same Gateway.

## 2. Rules That Shape the Plan

- Pre-existing project allowed only if "significantly updated" during the window. The whole
  hosted backend, the Fire TV app, and the shared-trip feature are new and must be demoed as
  new.
- Repo must be public with an OSI license before submission. Go public in week 1.
- Video under 3 minutes, must show the app running on a Fire TV device or Vega Virtual Device.
- Friction log earns up to 10% bonus. Keep one from day one.
- Testing instructions may include login credentials (see section 6).

## 3. Timeline (6.5 weeks)

| Week | Dates | Deliverable |
|------|-------|-------------|
| 1 | Sep 8 to 14 | CDK skeleton: Cognito+LWA, device-code Lambda, S3, DynamoDB, API, long-poll events. PWA uploader to S3 multipart. VVD + RN sample with fetch-only rule. Spike: native LWA and WebSocket on VVD, pass/fail only. Public repo, LICENSE. Apply Alexa+ preview. Friction log. |
| 2 | Sep 15 to 21 | Ingest Step Functions with Distributed Map: Rust Lambda with libheif/turbojpeg for decode, EXIF, pHash, sharpness, renditions; burst grouping and keeper pick; Rekognition and Titan on keepers; ffmpeg layer for video posters. |
| 3 | Sep 22 to 28 | Trip clustering, route build, Location static map, narrative via Bedrock. Trip codes, guest join, contributor attribution, cross-contributor dedup. Fire TV: onboarding QR, Invite QR, Memories home, Trip view. |
| 4 | Sep 29 to Oct 5 | Gateway Lambda targets, Identity, Runtime agent, Memory. W1, W2. Fire TV Ask screen, Review Changes modal. Per-trip embedding cache. |
| 5 | Oct 6 to 12 | W3, W4, W5, W6 clock alignment. Policy. Highlights, Ambient screens. Alexa+ simulator. |
| 6 | Oct 13 to 19 | Polish, Observability, video. Load a real multi-year archive. |
| 7 | Oct 20 to 23 | Submission text, test credentials for judges, friction log, feature requests. Submit Oct 22. |

Week 1 spike results decide the two upgrades in `architecture.md` section 3 (native LWA
sign-in, AppSync Events over WebSocket). Defaults ship either way.

## 4. Hardware and Accounts

- Fire TV Stick 4K Select (Vega) for real-device footage. VVD acceptable per rules.
- Fire TV Cube or 4K Max for the Fire OS build (optional, same codebase).
- AWS account with Bedrock model access (Claude, Titan, Nova) and AgentCore enabled in a
  supported region (us-east-1 or us-west-2, see open question 4).
- Alexa+ developer preview (apply week 1).
- A domain for the `link.` and `upload.` pages the TV QR codes point at.
- Demo Amazon account for judges (section 6).

## 5. Demo Video Beat Sheet (under 3 min)

1. 0:00 Fire TV first launch: code and QR on screen, phone scans, signs in, TV is in.
   "Install, scan, pick photos."
2. 0:20 Upload from the phone. Second phone scans the Invite QR; contributions tick up on the
   TV live.
3. 0:50 Memories home: new Kyoto trip appears. Trip Curator (W1) trace in Observability.
4. 1:10 Trip view: day timeline, route map, narrative with contributor credits.
5. 1:40 Ask: "show beach days from Portugal with the kids". Playlist plays.
6. 2:05 Dedup Steward (W4) proposes keepers across two phones. Review Changes modal, confirm
   on the remote, Policy allows.
7. 2:30 Alexa+ on Echo Show: "what did we do on day two in Kyoto". Same Gateway.
8. 2:45 Close: architecture card, privacy position.

Observability traces of W1 and W4 go in the video.

## 6. Judge Access

Rules allow login credentials in testing instructions. Provide a demo Amazon account
pre-loaded with a sample archive so judges can install the app on VVD or a Fire TV and see
populated memories without uploading.

## 7. Submission Assets

- Video (section 5).
- Testing instructions with the demo account (section 6).
- Friction log, kept from week 1.
- Feature requests: the missing Amazon Photos MCP tool and LWA `photos:read` scope, named as
  the single biggest ecosystem gap for Alexa+ and Fire TV memory apps (`architecture.md`
  section 2a).
- Public repo, MIT license, architecture card.
