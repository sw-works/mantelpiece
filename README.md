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

## Layout (planned)

```
apps/tv/         React Native for Vega, also builds for Fire OS
apps/web/        Phone uploader and sign-in approval pages
services/        Rust Lambdas: ingest, organize, API, tools
agents/          Strands agent for AgentCore Runtime
infra/           CDK stack
```

## License

MIT, see [LICENSE](LICENSE).
