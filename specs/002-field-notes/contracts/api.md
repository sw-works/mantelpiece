# API Contract: Field Notes

Changes to `specs/001-mantelpiece-baseline/contracts/api.md`. Access follows the existing
trip access matrix (owner, co-editor, contributor, member household, read-only code view).

## Changed

| Method | Path | Change |
|---|---|---|
| GET | /media/{id}/url | `rendition` also takes `fieldnote`. The server picks the form: the owner's household (TV, paired phone, owner session) gets `fieldnote.png`; a member household, a code-gated view or a shared link gets `fieldnote-public.png`. 404 when the photo has no field note. |
| GET | /trips/{id} | Each day's media entries and `highlights` gain `field_note: {placement, mode, certainty: "precise" \| "approximate", credit}` when one exists. `certainty` is always `approximate` for a member household or a code-gated view. |
| GET | /view?code= | Same `field_note` object on highlights, always the public form. |

## New

| Method | Path | Who | Behaviour |
|---|---|---|---|
| POST | /media/{id}/fieldnote/full | owner, co-editor of the trip | Composes the original with the owner-form field note at full resolution, stores `fieldnote-full.jpg`, returns `{url, expires_in}` (presigned GET). Reuses the stored object while the note's fingerprint is unchanged. 404 when the photo has no field note; 403 for anyone else. |

## Pipeline (internal)

| Step | Input | Output |
|---|---|---|
| Organize (changed) | `{task: "organize", household}` | Adds `field_notes_manifest`: S3 key of a JSON array of `{household, media_id}` for highlights whose note is missing or whose fingerprint changed. Drops notes (record and objects) for photos that stopped being highlights. Sets `Household.home`. |
| Field note (new) | `{task: "field_note", household, media_id}` | `{media_id, made: bool, skipped: Option<String>}`. Skips with a reason: `no place`, `no calm region`, `map data unavailable`, `not a highlight`, `up to date`. |

The organize state machine runs a Distributed Map over `field_notes_manifest` after
`OrganizeFinal`, at most four in flight per household, like `EnrichKeepers`.

## Events

| Type | Payload | Notes |
|---|---|---|
| field_notes.ready | trip, count | NEW. Emitted once per organize run that made at least one note, so a TV in the trip's slideshow can pick them up without restarting it. |
