# Devpost project overview

Submission: https://devpost.com/submit-to/30992-build-ship-shape-amazon-developer-hackathon/manage/submissions/1175821/project-overview

## Project name

Mantelpiece

## Tagline

Everyone's phone photos, on the family TV. Mantelpiece shows them as they arrive, turns each trip into a story with maps and short films, and plays live video from any phone.

## Track

Fire TV (primary). Alexa+ (secondary submission, same agent tools through an MCP add-on).
Mini-challenges: AWS Builder, Open Source.

## About the project

### What it does

Install Mantelpiece on a Fire TV and give your household a name. The TV then shows a QR code. Each person scans it with their phone, types their name, and someone on the sofa approves them with the remote. Nobody needs an account, and there is nothing to install on the phone. On a trip, everyone scans the trip's QR code from their own phone, and their photos go into the same trip with their name on them.

When a photo arrives, it fills the TV screen with the sender's name. One button on the remote sends "loved your photos" back to their phone as a notification. A phone can also ask to go live. When someone at home selects Watch, the phone's camera plays on the big screen about four seconds behind: a party, a school recital, or a trip on the other side of the world.

Mantelpiece organizes each trip on its own. It groups burst shots and keeps the best one, scores every photo, and lines up phones whose clocks disagree. It splits the trip into days, traces the route, and writes a day-by-day story that says who took which photos. Landmarks come with a short encyclopedia summary, credited on screen.

Then it makes things to watch:

- **Field notes.** Each highlight photo gets a small map of where it was taken, drawn from OpenStreetMap and terrain data. The map sits in the calmest, emptiest part of the photo, so it stays clear of the people and the waterfall. One button in the slideshow turns field notes on, and the map opens a page about the place.
- **Sketch maps.** A hand-drawn map of the whole trip and of each day: ink coastlines, watercolour sea and glaciers, pencil shading on the hills, and the route as a dashed ink line with circled day numbers. The slideshow starts each new day on its map.
- **Reels and day films.** The whole trip, or one day, as a music video on the TV. Every cut falls at the start of a bar of music, and each photo moves slowly toward its subject. At 21:00 each day, that day of the trip becomes a film the family can play from the sofa.

Grandma's Fire TV can join the same trip with its code. She gets everyone's photos the evening they are taken, and her reactions show up in the ticker on your TV. Ask the TV for "beach days with the kids" and it plays a matching playlist.

Deleting or sharing photos always needs a person. Only the owner or a co-editor can approve it, on the TV's Review Changes screen, and an AI agent cannot approve it on its own. Maps never show places near home precisely, and anything shared outside the household gets its maps without the part of the route near home.

### How we built it

- **Fire TV app:** React Native for Vega. The screens are shared in `packages/tv-ui` and sit inside a thin Vega shell. The app talks to the backend with fetch only and bundles no native modules; it uses the system-provided `@amazon-devices/*` ports for storage and media playback.
- **Phone pages:** a web app (PWA), so there is nothing to install. Uploads go straight to S3 in parts and pick up where they left off if the connection drops. A streaming SHA-256 hash rejects exact duplicates before any processing. On Android the page appears in the share menu, and "loved your photos" arrives as a web push notification (RFC 8291, written in Rust).
- **Backend:** Rust on AWS Lambda behind API Gateway. It handles QR pairing approved on the TV, HMAC-signed sessions, trip codes, presigned upload links, and a long-poll event feed that keeps the TV up to date.
- **Photo processing:** Step Functions. A task for each photo reads its EXIF data, computes a perceptual hash and a sharpness score, and makes resized copies. Then, for each household, it organizes the photos (bursts, best shots, clock alignment, trips, routes, highlights), sends the best shots to Rekognition, and uses Distributed Maps to draw field notes and sketch maps. Only the best shot of a burst goes to a model; the other shots reuse its results.
- **Maps:** drawn in Rust with tiny-skia from OpenFreeMap vector tiles and AWS Open Data terrain tiles. Contours, watercolour washes and hand-drawn lines come from seeded noise, so a map always redraws exactly the same, byte for byte. Amazon Nova Lite finds the subjects in each photo so the field note stays clear of them.
- **Reels and day films:** reels are planned without an AI model, from the photo scores, the subject boxes and the beat grid of CC BY music. Day films are rendered frame by frame in Rust, encoded with ffmpeg, and scheduled for each day with EventBridge Scheduler.
- **Live video:** Amazon IVS low-latency channels. The phone broadcasts from its browser, and the TV plays the HLS stream through Vega's W3C media port with a patched hls.js.
- **Agents:** Amazon Bedrock AgentCore. A Concierge agent built with Strands runs on AgentCore Runtime (Amazon Nova Pro by default) and writes each new trip's story when EventBridge reports the trip. Its tools go through AgentCore Gateway, with the API Lambda as the target. An interceptor locks every call to the caller's household, and Cedar policies block irreversible tools unless the call carries a confirmation token.
- **Observability and audit:** every agent run and tool call is recorded as a trace span tagged with the household. Every pairing, share, deletion and gated tool call leaves an audit record with the confirmation it carried.
- **One CDK stack:** `make up` deploys it and `make down` removes it. Everything under `make test` runs without AWS.

### Challenges

- Amazon Photos has no public API, so photos have to come from the phone's camera roll through the browser. This is the biggest gap for memory apps on Fire TV and Alexa+. Our friction log asks for an Amazon Photos MCP tool and a Login with Amazon `photos:read` scope.
- Live video on Vega needed three fixes in the player: wait for the IVS playlist before attaching media, and attach it only once; keep AAC-LC audio by telling hls.js which platform it is on; and rebuild the player when the picture stalls.
- Amazon Location's static maps cannot be stored under its terms, so we drew our own route maps: hand-drawn sketch maps from OpenStreetMap, with credit on every map.
- A trip story written by a model must never invent a place or a date. It is written only from the trip's structured data and checked against that data before it is saved. A story that fails the check is not shown.
- Finding subjects in landscape photos: the first prompt found nothing in a photo of sea stacks. Naming landscape subjects in the prompt, passing the trip's landmark as a hint, and reading the model's answer more leniently fixed it.

### What's next

Register the Alexa+ add-on when the preview opens; its OAuth 2.1 metadata, PKCE client and scope are already served. Then try the Echo Show slideshow skill with Alexa. Run Mantelpiece on a Fire TV Stick: so far everything has run on the Vega Virtual Device, where reel music stops after half a second and the reel plays on silently. After that: decoding HEIC photos on the server, native Login with Amazon on the TV, and S3 Vectors for search once it is available in our region. What we have not yet verified is listed in `docs/friction-log.md` and `specs/decisions-2026-09-09.md`.

## Built with

Rust, React Native, TypeScript, AWS Lambda, API Gateway, Step Functions, DynamoDB, S3,
CloudFront, EventBridge, EventBridge Scheduler, Cognito, Login with Amazon, Rekognition,
Amazon Bedrock (Amazon Nova), Bedrock AgentCore (Gateway, Runtime, Policy), Strands
Agents, Amazon IVS, AWS CDK, Vega OS, OpenStreetMap (OpenFreeMap), tiny-skia, ffmpeg.

## Links

- Repository: https://github.com/sw-works/mantelpiece
- Architecture: `specs/architecture.md`, diagrams in `docs/diagrams/`
- Testing instructions: `docs/testing.md`
- Friction log: `docs/friction-log.md`; Alexa+ add-on: `docs/alexa-plus.md`
