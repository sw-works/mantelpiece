# Alexa+ add-on

MemoryLane's tools reach Alexa+ through the same AgentCore Gateway the Concierge uses
(research R6, `specs/001-memorylane-baseline/contracts/tools.md`). Nothing Alexa-specific
runs in the backend: the add-on is an MCP client with an OAuth token for one household.

Status 2026-09-09: the stack and the API serve everything below; the registration itself
is not done, since the Alexa+ preview has not been granted to this account. Every step
marked *unverified* comes from the preview documentation, not from a run.

## What the stack provides

| Piece | Where | Value after `make deploy` |
|-------|-------|---------------------------|
| MCP endpoint | `CfnGateway` in `infra/lib/memorylane-stack.ts` | `GatewayUrl` output; Streamable HTTP, spec 2025-11-25 |
| Authorization server metadata | `GET /.well-known/oauth-authorization-server` on the API | RFC 8414 document pointing at the Cognito hosted UI |
| Protected resource metadata | `GET /.well-known/oauth-protected-resource` | RFC 9728 document naming the Gateway as the resource |
| OAuth client | Cognito app client `AlexaPlus`, no secret, PKCE required | client id in the Cognito console |
| Scope | Cognito resource server `memorylane`, scope `tools` | `memorylane/tools` |
| Tool policy | `infra/policy/tools.cedar` | irreversible tools deny without a TV-minted confirmation |

The redirect URI on the `AlexaPlus` client is a placeholder until the preview says which
to use; pass the real ones through the `alexaCallbackUrls` stack prop.

## Registration steps (unverified)

1. Apply for the Alexa+ developer preview and open the add-on console.
2. Create an add-on of type MCP, transport Streamable HTTP, endpoint = the `GatewayUrl`
   output.
3. Authentication: OAuth 2.0 authorization code with PKCE. Authorization URL
   `<OAUTH_DOMAIN>/oauth2/authorize`, token URL `<OAUTH_DOMAIN>/oauth2/token`, client id
   from the `AlexaPlus` app client, scope `memorylane/tools`. Both URLs are in the
   authorization server metadata document.
4. Copy the redirect URIs the console shows into `alexaCallbackUrls` and redeploy.
5. Register example phrases:
   - "what did we do on day two in Kyoto"
   - "show beach days from Portugal on the TV"
   - "what's that statue in the photo on my TV"
6. Test on an Echo Show linked to the same Amazon account as the household's TV. The
   first phrase reads the trip through `get_trip`; the second builds a playlist through
   `search_media` and `create_playlist` and the TV plays it from the feed; the third
   reads `get_now_showing` then `describe_photo`.

## Budget

Every read tool answers from stored data so the round trip stays under 500 ms:
descriptions, landmarks and captions are produced at enrich time and `describe_photo`
only calls a model when nothing is stored. Measure with the `tool.*` spans in AgentCore
Observability once deployed.

## Household mapping

The Gateway's JWT carries the Cognito subject; the API maps it to the household that
approved a TV with that account (`POST /device/approve`). An account without an approved
TV gets an empty trip list, never another household's data (F-602).

## Echo Show slideshow: the custom skill (2026-09-12)

Alexa+ add-ons have no screen surface and Amazon Photos' Photo Frame takes no third-party
feed, so trips reach an Echo Show through a classic custom skill in `agents/src/memorylane_skill`
(stdlib Python, one Lambda, `agents/skill/` holds the interaction model and manifest).

- "Alexa, ask Memory Lane to play the latest trip" or "... to show Skye": on a screen the
  skill renders an APL `Pager` of the trip's TV renditions (highlights first, then each
  day, 24 at most) with the trip name and day title as a caption, and an `AutoPage`
  command advances every 8 seconds. Without a screen it reads the trip's story, or the
  day titles when no story passed the check.
- "Alexa, open Memory Lane" lists the newest trips and waits for a name. Names match by
  word overlap, so "the Skye one" finds "Skye and Lochalsh, April 2025".
- Account linking: the `AlexaSkill` Cognito client (confidential, authorization code,
  profile scopes). The skill trades the linked token for an owner session with
  `POST /session/amazon`, cached per container for 20 minutes, so later calls carry no JWT.
  An unlinked or rejected token answers with the LinkAccount card.
- Screen time: the document asks for a 30-minute `idleTimeout`; the docs say the device
  may bound it, so how long the last slide stays is unverified. The session itself ends
  after the reply, which is what lets the slideshow keep the screen.

Registration (console, unverified): create the skill with `agents/skill/skill.json`, paste
`agents/skill/en-US.json` as the en-US model, set the endpoint to the `AlexaSkillFunctionArn`
output, and in Account Linking use the Cognito hosted UI (`/oauth2/authorize`,
`/oauth2/token`) with the `AlexaSkillClientId` output and its secret; copy the three
redirect URLs the console shows into `-c alexa-skill-callback-urls` and the skill id into
`-c alexa-skill-id`, then deploy again so the Lambda accepts only that skill.
