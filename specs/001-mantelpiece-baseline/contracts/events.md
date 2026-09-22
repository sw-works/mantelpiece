# Contract: Household Feed and EventBridge

## Household feed (`GET /events?since=`)

Sequenced per household; the TV and phone long-poll with the last `seq`. Existing kinds
are in `services/crates/core/src/events.rs`; NEW kinds are added by this feature.

| type | payload | shown as |
|------|---------|----------|
| phone.requested | contributor, display_name | NEW "Maya wants to connect a phone"; the TV opens its approval queue unless a show is playing |
| upload.progress | contributor, done, total | "Uploading 3 of 42" |
| contribution.received | trip?, contributor, display_name, count | "Maya added 42 photos from day 3" |
| trip.detected | trip, new_media | "New memory: 412 photos organised" |
| memory.ready | trip | "Memory ready" |
| playlist.created | playlist, name | "Playlist ready: …" |
| play.requested | trip?, playlist?, name, source, start? | NEW TV opens the slideshow once per request, at `start` when the phone cast one photo (F-405 by voice, "Play on TV") |
| photos.arrived | contributor, display_name, batch, media[], count | NEW family channel: Signal emits it as a phone's photos get renditions, one per photo up to 12, then one per dozen; `media` is the batch so far (at most 12). The TV opens Arrived over home or the idle slideshow when the event is under ten minutes old |
| live.started | broadcast, display_name, playback_url | NEW a phone went live; the TV plays the HLS URL (about 8 s behind the camera on Vega) |
| live.ended | broadcast | NEW from the phone or the TV |
| live.requested | request, display_name, expires_at, replacing? | NEW a phone asks to go live; the TV prompts until `expires_at` (60 s); `replacing` names whoever a grant would cut off |
| live.request_closed | request | NEW granted, declined or withdrawn; the TV drops the prompt |
| dedup.proposal | trip?, groups, removable | opens Review Changes |
| media.removed | media[] | refresh |
| cleanup.proposal | trip?, count, reasons | NEW opens Review Changes (F-705) |
| reaction.received | trip, target, actor_name, kind, text? | NEW "Grandma Ruth loved day 3" (F-710) |
| membership.joined | trip, member_name | NEW "Ruth's living room joined Kyoto" (F-162) |
| answer.card | question, answer, media | NEW card over the paused photo (F-405) |
| narration.ready | trip, day, audio_key | NEW (F-709, flagged) |
| share.join_code | trip, recipient, code | NEW W5 found the recipient has a TV; the code replaces a link (F-601) |
| memories.ready | date, on_this_day, then_and_now | NEW nightly (F-701, F-702) |

Rules: a member household's feed receives events for joined trips filtered to
contribution.received, memory.ready, reaction.received, media.removed.

## EventBridge (bus `mantelpiece`)

| detail-type | source | consumer |
|-------------|--------|----------|
| trip.detected | ingest organize | Concierge W1 Trip Curator |
| contribution.received | ingest organize | Concierge W6 Trip Merge |
| dedup.proposal | ingest organize | Concierge W4 Dedup Steward |
| cleanup.candidates | ingest enrich | NEW Concierge quick cleanup proposal |
| nightly.tick | scheduled rule 03:00 household local | NEW Concierge only: W3 Household Patterns, memories_nightly, and the purge_expired tool. No Lambda runs on a timer |
