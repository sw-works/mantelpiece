# Quickstart: proving the baseline works

Prerequisites: Node 22 with pnpm 10, Rust 1.93 with cargo-lambda, Python 3.12 with uv,
and for TV work the Vega SDK 0.24 with the Virtual Device. AWS credentials only for the
deploy and the deployed scenarios.

## Offline gate (every change)

```sh
make lint
make test
```

Expected: rustfmt, clippy, eslint, tsc, ruff clean; cargo, jest, vitest, pytest green with
no network. This is constitution VI and SC-011.

## TV app on the Vega Virtual Device (US1, US7 screens)

```sh
vega virtual-device start
vega device start-port-forwarding --device VirtualDevice -p 8081 --forward false
cd apps/tv-vega && pnpm start &
pnpm build:vega:debug
vega run-app build/aarch64-debug/*.vpkg com.hanyu.memorylane.main -d VirtualDevice
vega device start-log-stream --device VirtualDevice
```

Expected: Onboarding screen with code and QR; after phone approval, Memories home; after a
restart the TV is still signed in (R2).

Scripted walkthrough and screenshots: `make tv-walkthrough` (enables Developer Mode with
`vsm developer-mode enable`, drives `inputd-cli button_press KEY_DOWN` and friends). The
device screenshot tool `gwsi-tool-screenshooter` works only on a physical Fire TV; on the
Virtual Device run it with `SHOT=host` to capture the window from macOS.

## Deploy (once per environment)

```sh
export AWS_REGION=us-east-1   # R4
aws configure                  # once
(cd infra && pnpm cdk bootstrap)   # once per account and region
scripts/ffmpeg-layer.sh        # static ffmpeg for the poster Lambda
make deploy CDK_ARGS="-c amazon-client-id=… -c amazon-client-secret=…"   # LWA optional
make deploy-web                # builds apps/web against the outputs and publishes it
```

Expected outputs: `ApiUrl`, `LinkBase`, `UploadBase` (CloudFront unless `-c link-base` and
`-c upload-base` name custom domains), `CognitoDomain`, `UserPoolClientId`, `GatewayUrl`,
`AlexaSkillClientId`, `AlexaSkillFunctionArn`, `MediaBucket`, `WebBucket`.

## Scenarios by user story

| Story | Run | Expect |
|-------|-----|--------|
| US1 sign in | scan the TV QR with a phone, approve | TV on Memories home within 5 s; `GET /me` returns the household |
| US2 upload | pick 50 mixed files on the upload page | phone list reaches done; TV ticker shows "Uploading N of M"; thumbnails appear; re-uploading one file returns 409 duplicate |
| US3 shared trip | second phone scans the trip QR as guest "Maya", uploads | photos credited to Maya; ticker "Maya added N photos"; `DELETE /codes/{code}` then a new scan is refused |
| US4 organise | upload the 400-photo, 7-day sample archive | one trip, seven days, route map, 24 highlights; narrative contains only places and dates in the metadata (`narrative.check = passed`) |
| US5 ask | POST /search {"query":"the ramen place on day 1"} from the TV; hold the phone mic on a paused photo and ask about the statue | playlist plays; `answer.card` event and card on the TV under 3 s |
| US6 review | upload the same 20 photos from two phones | `dedup.proposal` event; Review Changes shows 20 before, 0 after; Keep everything changes nothing; Apply with the token removes 20, `restore_until` set |
| US7 second TV | second Virtual Device signed into a different account, Join a trip with the code | trip appears with "shared by" badge; a pin on TV 2 does not change TV 1; a reaction from TV 2's household shows on TV 1's ticker |
| US8 memories | load the multi-year sample archive; wait for the nightly rule or invoke `memories_nightly` | `GET /memories/home` has On this day and Then and now; `GET /map` has every trip |
| US9 people | POST /people/enable, name two clusters from the phone | `POST /search` with `{people:["Maya"]}` returns only her photos; disable deletes the collection |
| US10 reactions | open the read-only link, like day 3, comment | `reaction.received` within 30 s; comment beside the photo in Trip view |
| US11 ambient | leave the TV idle 5 minutes | ambient favours pinned, skips hidden; with the narration flag on, caption or audio in sync (R10) |
| US12 share | POST /shares with a confirmation | link opens the album; after `expires_at` it returns nothing |

## Agent traces (F-506)

Every workflow run appears in AgentCore Observability with the tool calls; W1 and W4
traces go into the demo video.
