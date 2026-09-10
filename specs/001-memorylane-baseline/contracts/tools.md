# Contract: Gateway Tools (MCP)

Exposed through AgentCore Gateway with the API Lambda as target. Same surface for the
Concierge (Strands) and Alexa+. Names match `agents/src/memorylane_agents/tools.py`; NEW
tools are added by this feature. Every call carries `household`; the Gateway's identity
maps the caller to a household and role. Alexa+ constraints (R6): every tool answers in
under 500 ms from stored data.

## Read tools

| Tool | Args | Returns |
|------|------|---------|
| list_trips | household | Trip summaries, including joined trips |
| get_trip | household, trip_id | Trip with days, route, narrative, highlights |
| get_trip_code | household, trip_id | NEW the trip's join code for another TV (W5) |
| search_media | household, query, trip_id?, filters? | media ids ranked |
| list_duplicates | household, trip_id? | DedupGroup[] |
| read_memory | household | preferences and patterns |
| get_now_showing | household, tv? | NEW `{media, trip, day, paused, at}` or none (R3) |
| describe_photo | household, media_id, question | NEW `{answer, landmark?, taken_by, when, related[]}`; answer checked against metadata (R8) |
| list_cleanup_candidates | household, trip_id? | NEW media with `cleanup_reason` (F-705) |
| memories_for | household, date? | NEW on-this-day, then-and-now, best-of sets (F-701 to F-703) |

## Write tools, reversible

| Tool | Args | Notes |
|------|------|-------|
| create_playlist | household, name, media, source | returns playlist id |
| save_narrative | household, trip_id, text, model_id | rejected when the fact check fails |
| write_memory | household, key, value | |
| push_event | household, event | kinds in events.md |
| set_keeper | household, media_id | NEW owner or co-editor; moves a burst group's keeper |
| synthesize_narration | household, trip_id, day | NEW Polly via `Speech`; emits `narration.ready` (F-709) |
| purge_expired | household | NEW deletes media past `restore_until`; called by `memories_nightly` |
| request_confirmation | household, action | opens a Proposal; returns token only if already confirmed on the TV |

## Write tools, irreversible (Cedar: token required)

| Tool | Args | Cedar rule |
|------|------|------------|
| apply_dedup | household, group_ids, confirmation | token.action = ApplyDedup(group_ids) |
| apply_cleanup | household, media_ids, confirmation | NEW token.action = ApplyCleanup(media_ids) |
| delete_media | household, media_ids, confirmation | token.action = DeleteMedia; or RemoveMedia with subject role co_editor or owner on the trip |
| share_album | household, playlist, name, confirmation, email? | token.action = ShareAlbum(playlist); `media` describes the album, the playlist is what is published |
| remove_contributor | household, contributor_id, confirmation | token.action = RemoveContributor; owner only |

Agents cannot call `/confirmations`; the token is minted only by a TV or phone session
belonging to an owner or co-editor, and is single use, ten minutes, bound to the exact
action payload.

## Alexa+ add-on

- Transport: Streamable HTTP, MCP spec 2025-11-25.
- Auth: OAuth 2.1 with PKCE against Cognito; protected resource metadata at
  `/.well-known/oauth-authorization-server`; scope `memorylane/tools`.
- Example phrases registered: "what did we do on day two in Kyoto", "show beach days from
  Portugal on the TV", "what's that statue in the photo on my TV".
