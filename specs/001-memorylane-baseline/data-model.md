# Data Model: MemoryLane Baseline

One DynamoDB table, partition key `pk = household id`, sort key `sk = <type>#<id>`.
Existing types live in `services/crates/core`; new ones are marked NEW. Blob keys are in S3.
Every record carries `created_at` and `updated_at`.

## Entities

### Household
- `id` (owner's Cognito `sub`), `name` (from the TV, e.g. "Hanyu's living room"),
  `people_enabled` (bool, default false, NEW), `narration_enabled` (bool, NEW),
  `region`, `created_at`.
- Owns everything below by `pk`. Deleting a household deletes every `sk`, the S3 prefix,
  the face collection, and the Memory namespace (F-604).

### TvSession (NEW, persisted on the TV; server side it is a DeviceCode outcome)
- `token`, `household`, `device_id`, `tv_name`. Stored on the TV via AsyncStorage (R2).
- Server: `sk = tv#<device_id>`: `last_seen`, `now_showing` (see Presence).

### DeviceCode (exists)
- `code` (8 chars), `device_id`, `state` pending | approved | expired, `expires_at`,
  `household` and `token` once approved.
- Transitions: pending → approved (phone approves) | expired (TTL). Only one approval wins.

### Contributor (exists)
- `id`, `kind` amazon | guest, `display_name`, `subject` (Cognito sub for amazon),
  `trip` (guest sessions are bound to one trip), `device` (guest), `role` (NEW:
  contributor | co_editor), `removed_at`, `grace_until`.
- Validation: a guest never has `role = co_editor` (F-164, F-165).

### TripCode (exists)
- `code`, `scope` household | trip, `trip`, `link`, `accepts_uploads`, `rotated_from`,
  `expires_at`. Rotating creates a new code and sets `accepts_uploads = false` on the old.

### Media (exists; fields added)
- `id`, `kind` photo | video, `contributor`, `original_key`, `checksum` (sha256, unique per
  household), `capture` {utc, original_zone, corrected_by}, `gps`, `camera`,
  `orientation`, `signals` {phash, sharpness, exposure}, `burst_group`, `is_keeper`,
  `renditions` {thumb, tv, poster}, `enrichment` {tags[], landmark?, faces_count,
  smiles, embedding_ref}, `score` 0..10, `state` uploaded | signalled | organised |
  enriched | removed, `pinned_by` (set of household ids, NEW for F-163), `hidden_by`
  (NEW), `text` (detected text, NEW, F-706), `description` {text, landmark, model_id,
  generated_at} (NEW, R8), `cleanup_reason` (screenshot | document | receipt | blurry,
  NEW, F-705), `removed_at`, `restore_until` (30 days).
- Transitions: uploaded → signalled → organised → enriched; any → removed (with
  confirmation) → restored (within `restore_until`) or purged.

### BurstGroup (exists as `burst_group` on Media)
- `id`, `keeper`, `members[]`, `contributors[]`. Exactly one keeper.

### Trip (exists; fields added)
- `id`, `name`, `start`, `end`, `hero`, `days[]` (TripDay), `route`, `narrative`
  {text, model_id, generated_at, check: passed | withheld}, `highlights[]`,
  `contributors[]`, `clock_offsets[]`, `owner_household` (NEW), `members[]` (NEW, see
  Membership), `code` (current trip code).
- Validation: `narrative.text` is null when `check = withheld` (F-301).

### TripDay (exists)
- `index`, `date`, `media[]`, `title`, `route` segment, `story` paragraph (NEW, for
  narration sync F-709).

### Membership (NEW, F-162)
- `pk = member household`, `sk = member#<owner household>#<trip>`: `role` member,
  `joined_at`, `left_at`. The trip is read from the owner's partition; nothing is copied.
- Reverse index item under the owner: `sk = shared#<trip>#<member household>` for the
  live contributor and member counts on the Invite screen.
- Validation: a household cannot join its own trip.

### Presence (NEW, R3)
- `sk = tv#<device_id>`: `now_showing` {media, trip, day, paused, at}. Overwritten on
  every slide change; `at` older than 10 minutes counts as nothing showing.

### Playlist (exists)
- `id`, `name`, `media[]`, `source` (ask:<query> | on_this_day:<date> | best_of:<period>
  | then_and_now | ambient), `created_at`.

### Album (exists) and ShareLink (NEW, F-601)
- ShareLink: `id`, `album` or `playlist`, `url`, `expires_at` (7 days default),
  `confirmation` id, `revoked_at`.

### Proposal (NEW, formalises F-410)
- `id`, `kind` dedup | cleanup | remove_media | remove_contributor | share,
  `action` (ConfirmedAction JSON), `before`, `after`, `samples[]`, `state` open |
  confirmed | cancelled | applied | expired, `requested_by` (agent | co_editor id),
  `confirmation` token id once minted.
- Transitions: open → cancelled | confirmed → applied; open → expired after 24 h.

### ConfirmationToken (exists; variants added)
- `token`, `action` one of ApplyDedup, DeleteMedia, ShareAlbum, RemoveContributor, NEW
  ApplyCleanup {media_ids}, NEW RemoveMedia {media_ids, by: contributor id} for
  co-editors, `subject` (who confirmed), `single_use`, `expires_at` (10 minutes).
- Validation: the Gateway's Cedar policy requires a valid token for every variant and,
  for RemoveMedia, that `subject` holds `co_editor` or owner on that trip.

### Reaction (NEW, F-710)
- `sk = reaction#<trip>#<day|media>#<actor>`: `kind` like | comment, `text` (comment, max
  200 chars), `actor` (contributor id or member household id), `created_at`.
- Feeds `reaction.received` events; one like per actor per target.

### Person (NEW, F-704)
- `sk = person#<id>`: `name` (nullable), `face_ids[]` (Rekognition), `media_count`,
  `sample_media[]`, `merged_into`. Exists only while `people_enabled`; deleted with the
  collection.

### FeedEvent (exists; kinds added)
- `seq`, `at`, `type`, payload. Kinds in contracts/events.md.

## Relationships

- Household 1..n TvSession, Contributor, TripCode, Media, Trip, Playlist, Person.
- Trip 1..n TripDay; Trip n..n Contributor; Trip 1..n Membership (member households).
- Media n..1 BurstGroup; Media n..1 Contributor; Media n..n Person.
- Proposal 1..1 ConfirmationToken (once confirmed); ShareLink 1..1 ConfirmationToken.

## Access rules (enforced in the API and mirrored in Cedar)

| Actor | Read | Write |
|-------|------|-------|
| Owner household (TV or phone session) | everything under `pk` and joined trips | everything; irreversible via token |
| Co-editor | the trip | rename, keepers, remove any media (token), cleanup (token) |
| Contributor (amazon or guest) | trips they joined (web view) | own uploads, own removals, reactions |
| Member household | joined trips | own pins and hides, reactions, leave |
| Agent via Gateway | per tool scope | proposals; irreversible only with token |
| Alexa+ via Gateway | per tool scope, same as agent | same as agent |
