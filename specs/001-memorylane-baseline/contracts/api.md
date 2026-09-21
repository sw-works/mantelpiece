# Contract: Backend API

HTTP over API Gateway, JSON bodies, bearer tokens issued by the API (TV, phone, upload
sessions) or Cognito JWTs (`/join/amazon`). Errors are `{ "error": "<message>" }` with
4xx/5xx. Routes marked NEW are added by this feature; the rest exist in
`services/lambdas/api/src/app.rs`.

## Sign-in and sessions (US1)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| POST | /tv/setup | none | NEW first launch: `{device_id, name}` creates the household under the name chosen on the TV; answers `{household, household_name, token}` (TV session) |
| PATCH | /household | TV only | NEW `{name}`; the household name is set on the TV and nowhere else |
| POST | /pair/code | TV only | NEW `{device_id}`; issues `{code, link, expires_at}`, link `/?p=<code>`, ten minutes, many phones per code |
| GET | /pair/code/{code} | none | NEW `{tv_name}` for "Connect to Living room" |
| POST | /pair/requests | none | NEW `{code, name}`; queues the phone, emits `phone.requested`, answers `{request, name, tv_name, token}` where the token only reads the status; 429 past 12 waiting |
| GET | /pair/status | waiting token | NEW held like the feed; `{state}` pending, denied, or approved with `{token, household, tv_name, name}` |
| GET | /pair/requests | TV only | NEW `{pending, phones}`, each `{id, name, at}`, oldest first |
| POST | /pair/requests/{id}/approve, /deny | TV only | NEW Allow, Not now, or Forget for a phone let in before; a forgotten phone's session answers 401 from then on |
| POST | /device/code | none | issues `{code, link, expires_at, poll_seconds}` |
| POST | /device/approve | phone (Cognito JWT) | `{code}`; first approval wins |
| GET | /device/poll?code=&device= | TV | `{state}` pending, expired, or approved with `{household, token}` |
| POST | /session/amazon | Alexa skill (Cognito JWT) | trades an account-linked token for an owner session: `{household, household_name, token}`; creates the owner contributor on first sign-in |
| GET | /me | any | session summary |

## Codes and joining (US3, US7)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| POST | /household/code | owner | household-scoped upload code |
| POST | /trips/{trip}/code | owner | trip-scoped code (Invite QR); returns live `contributors` and `member_households` counts |
| POST | /codes/{code}/rotate, DELETE /codes/{code} | owner | rotate or revoke |
| GET | /codes | owner | list |
| POST | /join, /join/amazon | guest / Cognito JWT | `{code, display_name?}` → upload session |
| POST | /memberships | owner TV | NEW `{code}` joins another household's trip (F-162); refuses own trip |
| DELETE | /memberships/{trip} | member household | NEW leave |
| GET | /trips also lists joined trips with `shared_by` | owner TV | NEW field |
| GET | /trips/{trip} carries `landmarks` (names) and per-day `landmarks` objects `{name, at, distance_m, source, kind?, dim_m?, media[]}`; route points carry `estimated` | all | NEW (F-206) |

## Uploads (US2)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| POST | /uploads/init | contributor | `{filename, size, sha256}`; duplicate → 409 |
| POST | /uploads/complete, /uploads/abort | contributor | multipart lifecycle |
| GET | /uploads/mine | contributor | NEW own uploads with state (phone "my uploads") |
| DELETE | /media/{media} | contributor (own) / owner / co-editor (token) | cascading removal; soft, `restore_until` 30 days |
| POST | /media/{media}/restore | owner, co-editor | NEW within `restore_until` |
| POST | /uploads/init returns 410 while the household is `deleting` | | edge case |

## Trips, media, highlights (US4)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| GET | /trips, /trips/{trip} | owner, member, contributor (web view) | trip with days, route, narrative, highlights |
| GET | /media/{media}/url?rendition=thumb\|tv\|poster | any with access | presigned URL |
| POST | /media/{media}/pin, /hide | owner or member household | per household (F-163) |
| POST | /trips/{trip}/narrative/regenerate | owner | NEW (F-302) |
| GET | /trips/{trip}/map?day= | owner, member, contributor (web view) | NEW presigned URL for the trip map or one day's map (F-207) |
| POST | /media/{media}/keeper | owner, co-editor | NEW moves the burst group's keeper; non-keepers re-inherit (F-201, F-164) |
| PATCH | /trips/{trip} | owner, co-editor | NEW rename |
| POST | /trips/{trip}/roles | owner | NEW `{contributor, role}` promote or demote (F-164) |
| POST | /trips/{trip}/play | owner, member household (phone, Alexa skill) | NEW `{source?, start?}`; emits `play.requested` to the caller's household, the TV opens the slideshow at `start` (a media id of the trip, else 400). Contributors and co-editors 403 |

## Search, playlists, Ask (US5)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| POST | /search | owner, member | NEW `{query, trip?, filters}` → media ids (F-303) |
| POST | /playlists | owner | NEW from media ids or a source |
| GET | /playlists, /playlists/{id} | owner, member | NEW |
| POST | /playlists/{id}/play | owner | NEW `{source?}`; emits `play.requested` for the playlist |
| PUT | /presence | TV | NEW `{media, trip, day, paused}` now-showing (R3) |
| POST | /ask | owner (phone or TV) | NEW `{question, tv?}` → `{card | playlist}`; target TV = `tv` param, else the presence record with the newest `at` (F-405) |

## Proposals and confirmations (US6)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| GET | /proposals | owner, co-editor | NEW open proposals for Review Changes |
| POST | /confirmations | owner, co-editor | `{action}` → single-use token, bound to caller |
| POST | /proposals/{id}/cancel | owner, co-editor | NEW |

## Reactions (US10)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| POST | /trips/{trip}/reactions | contributor, member | NEW `{target: day\|media, id, kind, text?}` |
| GET | /trips/{trip}/reactions | any with access | NEW |

## People (US9)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| POST | /people/enable, /people/disable | owner | NEW; disable deletes the collection |
| GET | /people | owner | NEW clusters with samples |
| PATCH | /people/{id} | owner (phone) | NEW `{name}` or `{merge_into}` |

## Memories (US8)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| GET | /memories/home | owner, member | NEW on-this-day, then-and-now, best-of rows |
| GET | /map | owner | NEW static map key and trip pins (F-707) |

## Sharing (US12)

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| POST | /shares | owner (token) | NEW `{album\|playlist, email?, confirmation}` → link |
| GET | /s/{share} | public | NEW read-only album until expiry |

## Feed

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| GET | /events?since= | TV, phone | long-poll, 25 s hold; kinds in events.md |

## Contributors

| Method | Path | Caller | Notes |
|--------|------|--------|-------|
| GET | /contributors | owner | |
| DELETE | /contributors/{id} | owner (token) | detach, grace period, export link |
