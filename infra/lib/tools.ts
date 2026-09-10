/**
 * Gateway tool definitions (contracts/tools.md). One MCP tool per row, all served by the
 * API Lambda. The Concierge and Alexa+ see the same list; the Cedar policy in
 * `policy/tools.cedar` gates the irreversible ones.
 */

interface Schema {
  type: string;
  description?: string;
  properties?: Record<string, Schema>;
  required?: string[];
  items?: Schema;
}

export interface ToolDefinition {
  name: string;
  description: string;
  inputSchema: Schema;
}

const str = (description: string): Schema => ({ type: "string", description });
const int = (description: string): Schema => ({ type: "integer", description });
const bool = (description: string): Schema => ({ type: "boolean", description });
const strings = (description: string): Schema => ({ type: "array", description, items: { type: "string" } });

const HOUSEHOLD = str("Household id the caller belongs to");
const TRIP = str("Trip id");
const CONFIRMATION = str("Confirmation token minted by the TV's Review Changes modal");

function tool(name: string, description: string, properties: Record<string, Schema>, required: string[]): ToolDefinition {
  return { name, description, inputSchema: { type: "object", properties, required } };
}

/** Read tools: answer from stored data, under 500 ms for Alexa+. */
export const readTools: ToolDefinition[] = [
  tool("list_trips", "Trip summaries for the household, including trips joined from another TV", { household: HOUSEHOLD }, ["household"]),
  tool("get_trip", "One trip with its days, route, narrative and highlights", { household: HOUSEHOLD, trip_id: TRIP }, ["household", "trip_id"]),
  tool(
    "search_media",
    "Media ids ranked for a natural-language query, optionally inside one trip or filtered by people",
    { household: HOUSEHOLD, query: str("What to look for"), trip_id: TRIP, people: strings("Person names to require") },
    ["household", "query"],
  ),
  tool("list_duplicates", "Near-duplicate groups with the keeper of each", { household: HOUSEHOLD, trip_id: TRIP }, ["household"]),
  tool("read_memory", "The household's preferences and patterns", { household: HOUSEHOLD }, ["household"]),
  tool(
    "get_now_showing",
    "What the household's TV is showing right now: media, trip, day, paused. Use this to resolve 'that photo' or 'the statue on my TV'",
    { household: HOUSEHOLD, tv: str("Device id of a specific TV; omit for the freshest") },
    ["household"],
  ),
  tool(
    "describe_photo",
    "Answer a question about one photo from its stored description and metadata: landmark, who took it, when, related photos",
    { household: HOUSEHOLD, media_id: str("Media id, usually from get_now_showing"), question: str("The user's question") },
    ["household", "media_id", "question"],
  ),
  tool("list_cleanup_candidates", "Screenshots, documents, receipts and blurry frames flagged for quick cleanup", { household: HOUSEHOLD, trip_id: TRIP }, ["household"]),
  tool("memories_for", "On this day, then and now, and best-of sets for a date", { household: HOUSEHOLD, date: str("ISO date; defaults to today") }, ["household"]),
];

/** Reversible writes. */
export const writeTools: ToolDefinition[] = [
  tool(
    "create_playlist",
    "Save a playlist of media ids; returns the playlist id",
    { household: HOUSEHOLD, name: str("Display name"), media: strings("Media ids in order"), source: str("Query or rationale") },
    ["household", "name", "media", "source"],
  ),
  tool(
    "save_narrative",
    "Save a trip story that passed the place-and-date check; rejected otherwise",
    { household: HOUSEHOLD, trip_id: TRIP, text: str("The story"), model_id: str("Model that wrote it") },
    ["household", "trip_id", "text", "model_id"],
  ),
  tool("write_memory", "Remember a preference or pattern", { household: HOUSEHOLD, key: str("Key"), value: str("JSON value") }, ["household", "key", "value"]),
  tool("push_event", "Put an event on the household feed", { household: HOUSEHOLD, event: str("Event JSON with a type field") }, ["household", "event"]),
  tool("set_keeper", "Make this media the keeper of its burst group (owner or co-editor)", { household: HOUSEHOLD, media_id: str("Media id") }, ["household", "media_id"]),
  tool(
    "synthesize_narration",
    "Turn one day's story into narration audio; emits narration.ready",
    { household: HOUSEHOLD, trip_id: TRIP, day: int("Day index") },
    ["household", "trip_id", "day"],
  ),
  tool("purge_expired", "Delete media whose restore window has closed", { household: HOUSEHOLD }, ["household"]),
  tool(
    "request_confirmation",
    "Open a proposal on the TV's Review Changes screen; returns a token only once the user confirmed",
    { household: HOUSEHOLD, action: str("ConfirmedAction JSON"), before: int("Count before"), after: int("Count after"), requested_by: str("agent or a co-editor id") },
    ["household", "action"],
  ),
];

/** Irreversible writes: Cedar requires `confirmation`. */
export const irreversibleTools: ToolDefinition[] = [
  tool("apply_dedup", "Remove the non-keepers of the given duplicate groups", { household: HOUSEHOLD, group_ids: strings("Group ids"), confirmation: CONFIRMATION }, ["household", "group_ids", "confirmation"]),
  tool("apply_cleanup", "Remove the given cleanup candidates", { household: HOUSEHOLD, media_ids: strings("Media ids"), confirmation: CONFIRMATION }, ["household", "media_ids", "confirmation"]),
  tool(
    "delete_media",
    "Remove media; restorable for 30 days",
    { household: HOUSEHOLD, media_ids: strings("Media ids"), confirmation: CONFIRMATION, by: str("Co-editor contributor id when not the owner") },
    ["household", "media_ids", "confirmation"],
  ),
  tool(
    "share_album",
    "Create an expiring share link for an album",
    { household: HOUSEHOLD, media: strings("Media ids"), name: str("Album name"), confirmation: CONFIRMATION, email: str("Recipient email, optional") },
    ["household", "media", "name", "confirmation"],
  ),
  tool(
    "remove_contributor",
    "Remove a contributor and, after a grace period, their photos (owner only)",
    { household: HOUSEHOLD, contributor_id: str("Contributor id"), confirmation: CONFIRMATION, requested_by: str("Must be owner"), unused: bool("Reserved") },
    ["household", "contributor_id", "confirmation"],
  ),
];

export const toolDefinitions: ToolDefinition[] = [...readTools, ...writeTools, ...irreversibleTools];
