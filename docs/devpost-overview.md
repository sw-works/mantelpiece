# Devpost project overview

Submission: https://devpost.com/submit-to/30992-build-ship-shape-amazon-developer-hackathon/manage/submissions/1175821/project-overview

## Project name

Mantelpiece

## Tagline

The family TV, shared with everyone's phones: photos the moment they land, the trip that builds itself, and a live camera from wherever they are.

## Track

Fire TV (primary). Alexa+ (secondary submission, same agent tools through an MCP add-on).
Mini-challenges: AWS Builder, Open Source.

## About the project

### What it does

Install Mantelpiece on a Fire TV, name the household, and the TV shows a QR. Each phone
scans it, gives a name, and someone on the sofa approves it with the remote. No account,
nothing to install on the phone. Everyone on a trip scans the trip's QR from their own
phone, and their photos land in the same trip, credited to them.

Photos take over the TV the moment they land, with the sender's name. One button on the
remote sends back "loved your photos" as a notification on their phone. A phone can ask to
go live: someone on the TV says Watch, and the camera from the party, the recital or the
other side of the world plays on the big screen about four seconds behind.

The trip builds itself. Mantelpiece groups burst shots and picks the keeper, scores every
photo, lines up the clocks of different phones, splits the trip into days, traces the
route and writes a day-by-day story that names who took what. Landmarks come with their
encyclopedia summary, credited on screen.

Then it makes something to watch:

- **Field notes.** Each highlight gets a quiet map of where it was taken, drawn from
  OpenStreetMap and terrain data into the photo's calmest empty space, clear of the people
  and the waterfall in it. One button in the slideshow turns them on; the map opens a page
  about the place.
- **Sketch maps.** A hand-drawn map of the whole trip and of each day: ink coastline,
  watercolour sea and glaciers, pencil hachures on the hills, the route in dashed ink with
  circled day numbers. The slideshow opens each new day on its map.
- **Reels and day films.** The trip, or one day, as a music video on the TV, every cut on a
  bar line and every photo moving toward its subject. At 21:00 each day of a trip becomes
  a film the family can play from the sofa.

Grandma's Fire TV joins the same trip with its code and gets everyone's photos the evening
they are taken; her reactions come back to the ticker on yours. Ask the TV for "beach days
with the kids" and a playlist plays. An agent keeps the archive tidy, and nothing
irreversible happens until someone confirms on the remote: only the Review Changes screen
on the TV can mint the token that lets a delete or a share run. Places near home are never
drawn precisely, and anything shared outside the household gets the maps without the home
end of the route.

### How we built it

- **Fire TV app** in React Native for Vega: shared screens in `packages/tv-ui`, a thin Vega
  shell, fetch only, no bundled native modules (system-deployed `@amazon-devices/*` ports
  for storage and media playback).
- **Phone pages** as a PWA: resumable multipart uploads straight to S3, streaming SHA-256
  for exact-duplicate rejection before any processing, a share-sheet target on Android,
  web push (RFC 8291 in Rust) for "loved your photos".
- **Backend in Rust** on Lambda behind API Gateway: QR pairing approved on the TV,
  HMAC-signed sessions, trip codes, presigned uploads, a long-poll event feed.
- **Ingest as Step Functions**: a per-photo task (EXIF, perceptual hash, sharpness,
  renditions), then per household: organize (bursts, keepers, clock alignment, trips,
  routes, highlights), enrich keepers with Rekognition, then Distributed Maps for field
  notes and sketch maps. Only keepers reach a model; the rest of a burst inherits.
- **Maps drawn in Rust** with tiny-skia from OpenFreeMap vector tiles and AWS Open Data
  terrain tiles: contours, watercolour washes and hand-drawn lines from seeded noise, so a
  map redraws byte-identical. Amazon Nova Lite finds the photo's subjects so the field
  note stays clear of them.
- **Reels** planned without a model from ingest's scores, subject boxes and the beat grid
  of CC BY music; **day films** rendered frame by frame in Rust and encoded by an ffmpeg
  layer, scheduled per day with EventBridge Scheduler.
- **Live** through Amazon IVS low-latency channels: the phone broadcasts from the browser,
  the TV plays HLS through Vega's W3C media port with a patched hls.js.
- **Agents on Bedrock AgentCore**: a Strands Concierge on Runtime with Memory (Amazon Nova
  Pro by default), six workflows (Trip Curator, Ask Your Archive, Household Patterns,
  Dedup Steward, Share, Trip Merge), tools through AgentCore Gateway with Lambda targets,
  and Cedar policies that deny irreversible tools without the TV's confirmation token.
- **Observability and audit**: every agent run and tool call is a span tagged with the
  household; every pairing, share, deletion and gated tool leaves an audit entry with the
  confirmation it carried.
- One CDK stack, deployed with `make up` and removed with `make down`. Everything under
  `make test` runs without AWS.

### Challenges

- Amazon Photos has no public API, so photos come from the phone's camera roll through the
  browser. This is the biggest gap for memory apps on Fire TV and Alexa+; the friction log
  asks for an Amazon Photos MCP tool and an LWA `photos:read` scope.
- Live video on Vega took three fixes in the player: waiting for the IVS playlist before
  attaching media once, keeping AAC-LC by naming the platform to hls.js, and rebuilding the
  player when the picture stalls.
- Amazon Location static maps cannot be stored under its terms, so the route maps became
  our own: hand-drawn sketch maps from OpenStreetMap, with credit on every map.
- A trip story from a model must never invent a place or date. It is generated from
  structured metadata only and checked against it before it is saved; a story that fails
  the check is withheld.
- Finding subjects in landscape photos: the first prompt boxed nothing in a photo of sea
  stacks. Naming landscape subjects, passing the trip's landmark as a hint and parsing the
  answer leniently fixed it.

### What's next

Register the Alexa+ add-on when the preview opens (the OAuth 2.1 metadata, PKCE client and
scope are already served) and run the Echo Show slideshow skill against Alexa. Run on a
Fire TV Stick: everything so far ran on the Vega Virtual Device, where reel music stalls
after half a second and plays silent. HEIC decoding on the server, native Login with
Amazon on the TV, and S3 Vectors for search once it is in the region. What stayed
unverified is listed in `docs/friction-log.md` and `specs/decisions-2026-09-09.md`.

## Built with

Rust, React Native, TypeScript, AWS Lambda, API Gateway, Step Functions, DynamoDB, S3,
CloudFront, EventBridge, EventBridge Scheduler, Cognito, Login with Amazon, Rekognition,
Amazon Bedrock (Amazon Nova), Bedrock AgentCore (Gateway, Runtime, Memory, Policy), Strands
Agents, Amazon IVS, AWS CDK, Vega OS, OpenStreetMap (OpenFreeMap), tiny-skia, ffmpeg.

## Links

- Repository: https://github.com/sw-works/mantelpiece
- Architecture: `specs/architecture.md`, diagrams in `docs/diagrams/`
- Testing instructions: `docs/testing.md`
- Friction log: `docs/friction-log.md`; Alexa+ add-on: `docs/alexa-plus.md`
