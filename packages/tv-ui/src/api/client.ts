// TV-side client. Vega-first rule: fetch, timers, images over HTTPS, nothing native.

export interface DeviceCodeIssued {
  code: string;
  link: string;
  expires_at: string;
  poll_seconds: number;
}

export type DevicePoll =
  | { state: "pending" }
  | { state: "expired" }
  | { state: "approved"; household: string; token: string };

export interface TripSummary {
  id: string;
  name: string;
  start: string;
  end: string;
  hero: string | null;
  highlights: string[];
  contributors: string[];
  /** Set on trips joined from another TV: the owning household's name (F-162). */
  shared_by?: string | null;
  owner_household?: string;
}

export interface TripDay {
  index: number;
  date: string;
  media: string[];
  title: string | null;
  route: { points: unknown[]; inferred: boolean; map_key: string | null };
  /** This day's paragraph of the story (F-709). */
  story?: string | null;
  /** Photos without a capture time, placed here by upload time. */
  undated?: string[];
}

export interface ClockOffset {
  contributor: string;
  seconds: number;
  samples: number;
}

export interface Trip extends TripSummary {
  days: TripDay[];
  narrative: { text: string; model_id: string; generated_at: string; check?: "passed" | "withheld" } | null;
  route: { points: unknown[]; inferred: boolean; map_key: string | null };
  /** Contributor id to display name (F-154). */
  contributor_names?: Record<string, string>;
  /** Contributor id to owner | co_editor | contributor (F-164). */
  contributor_roles?: Record<string, "owner" | "co_editor" | "contributor">;
  /** Media id to contributor id (F-154). */
  media_credits?: Record<string, string>;
  /** Whose clock was corrected, by how much (F-155). */
  clock_offsets?: ClockOffset[];
}

/** Display name for a media item's contributor, or null when unknown. */
export function creditFor(trip: Trip, mediaId: string): string | null {
  const contributor = trip.media_credits?.[mediaId];
  if (!contributor) return null;
  return trip.contributor_names?.[contributor] ?? contributor;
}

/** The story to show, or null while it is being written or was withheld (F-301). */
export function storyOf(trip: Trip): string | null {
  const n = trip.narrative;
  if (!n || n.check === "withheld" || !n.text) return null;
  return n.text;
}

/** Every media id of the trip in day order, for the slideshow. */
export function slides(trip: Trip): Array<{ media: string; day: number }> {
  return trip.days.flatMap((d) => d.media.map((media) => ({ media, day: d.index })));
}

/** "Ken's clock corrected by 3 min" lines for offsets over a minute (F-155). */
export function clockHints(trip: Trip): string[] {
  return (trip.clock_offsets ?? [])
    .filter((o) => Math.abs(o.seconds) >= 60)
    .map((o) => `${trip.contributor_names?.[o.contributor] ?? o.contributor}'s clock corrected by ${Math.round(Math.abs(o.seconds) / 60)} min`);
}

export interface SearchHit {
  media: string;
  score: number;
  trip: string | null;
  contributor: string;
}

export interface Playlist {
  id: string;
  name: string;
  source: string;
  media: string[];
  created_at: string;
}

export type AskResult =
  | { mode: "describe"; tv: string; question: string; card: { answer: string; landmark: string | null; taken_by: string; when: string | null; related: string[]; media: string } }
  | { mode: "search"; question: string; playlist: string; count: number }
  | { mode: "nothing"; question: string; hint: string };

/** A playlist as something the Slideshow can play: one day holding every item. */
export function playlistAsTrip(name: string, media: string[]): Trip {
  return {
    id: `playlist:${name}`,
    name,
    start: "",
    end: "",
    hero: media[0] ?? null,
    highlights: media,
    contributors: [],
    days: [{ index: 1, date: "", media, title: name, route: { points: [], inferred: false, map_key: null } }],
    narrative: null,
    route: { points: [], inferred: false, map_key: null },
  };
}

export interface ProposalSample {
  keep: string | null;
  remove: string[];
  reason: string;
}

/** A pending change waiting for the remote (F-410). */
export interface ProposalView {
  id: string;
  kind: "dedup" | "cleanup" | "remove_media" | "remove_contributor" | "share";
  action: Record<string, unknown>;
  trip: string | null;
  before: number;
  after: number;
  samples: ProposalSample[];
  state: "open" | "cancelled" | "confirmed" | "applied" | "expired";
  requested_by: string;
  expires_at: string | null;
}

export function proposalTitle(p: ProposalView): string {
  const n = Math.max(0, p.before - p.after);
  switch (p.kind) {
    case "dedup":
      return `Remove ${n} duplicate${n === 1 ? "" : "s"}, keeping the best of each burst`;
    case "cleanup":
      return `Tidy away ${n} screenshot${n === 1 ? "" : "s"}, receipts or blurry frames`;
    case "remove_media":
      return `Remove ${n} photo${n === 1 ? "" : "s"} (asked by ${p.requested_by})`;
    case "remove_contributor":
      return "Remove a contributor and their photos";
    case "share":
      return "Share an album outside the household";
  }
}

export interface MemoriesHomeView {
  date: string;
  on_this_day: string[];
  then_and_now: Array<{ then: string; now: string; lat: number; lon: number; years_apart: number }>;
  best_of_month: { year: number; month: string; media: string[] };
  best_of_year: { year: number; media: string[] };
}

export interface MapView {
  url: string | null;
  pins: Array<{ trip: string; name: string; lat: number; lon: number; start: string }>;
}

export interface PersonView {
  id: string;
  name: string | null;
  media_count: number;
  sample_media: string[];
  faces: number;
}

export interface ReactionsView {
  reactions: unknown[];
  summary: Record<string, { likes: number; comments: number; latest_comment: { by: string; text: string } | null }>;
}

export interface FeedEvent {
  seq: number;
  at: string;
  type: string;
  [key: string]: unknown;
}

export interface CodeView {
  code: string;
  scope: "household" | "trip";
  trip: string | null;
  link: string;
  accepts_uploads: boolean;
  contributors?: number;
  member_households?: number;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export class TvApi {
  constructor(
    readonly base: string,
    private readonly fetchFn: typeof fetch = fetch,
  ) {}

  private async call<T>(method: "GET" | "POST" | "PUT" | "DELETE", path: string, body?: unknown, bearer?: string): Promise<T> {
    const headers: Record<string, string> = {};
    if (body !== undefined) headers["content-type"] = "application/json";
    if (bearer) headers["authorization"] = `Bearer ${bearer}`;
    const res = await this.fetchFn(`${this.base}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (res.status === 204) return undefined as T;
    const json = (await res.json().catch(() => ({}))) as { error?: string };
    if (!res.ok) throw new ApiError(res.status, json.error ?? `HTTP ${res.status}`);
    return json as T;
  }

  issueDeviceCode(deviceId: string, tvName?: string) {
    return this.call<DeviceCodeIssued>("POST", "/device/code", { device_id: deviceId, tv_name: tvName });
  }

  pollDeviceCode(code: string, deviceId: string) {
    const q = new URLSearchParams({ code, device_id: deviceId });
    return this.call<DevicePoll>("GET", `/device/poll?${q.toString()}`);
  }

  events(token: string, since: number) {
    return this.call<{ events: FeedEvent[]; next: number }>("GET", `/events?since=${since}`, undefined, token);
  }

  trips(token: string) {
    return this.call<TripSummary[]>("GET", "/trips", undefined, token);
  }

  trip(token: string, id: string) {
    return this.call<Trip>("GET", `/trips/${id}`, undefined, token);
  }

  mediaUrl(token: string, mediaId: string, rendition: "thumb" | "tv" = "tv") {
    return this.call<{ url: string; expires_in: number }>("GET", `/media/${mediaId}/url?rendition=${rendition}`, undefined, token);
  }

  householdCode(token: string) {
    return this.call<CodeView>("POST", "/household/code", undefined, token);
  }

  tripCode(token: string, tripId: string, expiresInDays = 30) {
    return this.call<CodeView>("POST", `/trips/${tripId}/code`, { expires_in_days: expiresInDays }, token);
  }

  codes(token: string) {
    return this.call<CodeView[]>("GET", "/codes", undefined, token);
  }

  /** Preview a trip by its code before joining (the read-only view). */
  previewTrip(code: string) {
    return this.call<{ household_name: string | null; trip: Trip }>("GET", `/view?code=${encodeURIComponent(code)}`);
  }

  joinTrip(token: string, code: string) {
    return this.call<{ joined: boolean; trip: string; owner: string; name?: string; already?: boolean }>("POST", "/memberships", { code }, token);
  }

  leaveTrip(token: string, tripId: string) {
    return this.call<void>("DELETE", `/memberships/${tripId}`, undefined, token);
  }

  setRole(token: string, tripId: string, contributor: string, role: "co_editor" | "contributor") {
    return this.call<{ contributor: string; role: string }>("POST", `/trips/${tripId}/roles`, { contributor, role }, token);
  }

  rotateCode(token: string, code: string) {
    return this.call<CodeView>("POST", `/codes/${code}/rotate`, undefined, token);
  }

  mintConfirmation(token: string, action: Record<string, unknown>, proposal?: string) {
    return this.call<{ token: string; tool: string; expires_at: number; proposal?: string | null }>("POST", "/confirmations", { ...action, proposal }, token);
  }

  proposals(token: string) {
    return this.call<ProposalView[]>("GET", "/proposals", undefined, token);
  }

  cancelProposal(token: string, id: string) {
    return this.call<ProposalView>("POST", `/proposals/${id}/cancel`, undefined, token);
  }

  tripMap(token: string, tripId: string, day?: number) {
    const q = day === undefined ? "" : `?day=${day}`;
    return this.call<{ url: string; inferred: boolean }>("GET", `/trips/${tripId}/map${q}`, undefined, token);
  }

  regenerateNarrative(token: string, tripId: string) {
    return this.call<{ requested: boolean }>("POST", `/trips/${tripId}/narrative/regenerate`, undefined, token);
  }

  setKeeper(token: string, mediaId: string) {
    return this.call<{ keeper: string; changed?: boolean }>("POST", `/media/${mediaId}/keeper`, undefined, token);
  }

  search(token: string, query: string, trip?: string) {
    return this.call<{ query: string; results: SearchHit[] }>("POST", "/search", { query, trip }, token);
  }

  playlist(token: string, id: string) {
    return this.call<Playlist>("GET", `/playlists/${id}`, undefined, token);
  }

  playlists(token: string) {
    return this.call<Playlist[]>("GET", "/playlists", undefined, token);
  }

  narrationUrl(token: string, tripId: string, day: number) {
    return this.call<{ url: string; day: number }>("GET", `/trips/${tripId}/narration?day=${day}`, undefined, token);
  }

  putPresence(token: string, presence: { device_id: string; media: string; trip?: string | null; day?: number | null; paused: boolean }) {
    return this.call<void>("PUT", "/presence", presence, token);
  }

  ask(token: string, question: string, tv?: string) {
    return this.call<AskResult>("POST", "/ask", { question, tv }, token);
  }

  reactions(token: string, tripId: string) {
    return this.call<ReactionsView>("GET", `/trips/${tripId}/reactions`, undefined, token);
  }

  people(token: string) {
    return this.call<{ people_enabled: boolean; people: PersonView[] }>("GET", "/people", undefined, token);
  }

  setPeople(token: string, enabled: boolean) {
    return this.call<{ people_enabled: boolean }>("POST", enabled ? "/people/enable" : "/people/disable", undefined, token);
  }

  memoriesHome(token: string, date?: string) {
    return this.call<MemoriesHomeView>("GET", `/memories/home${date ? `?date=${date}` : ""}`, undefined, token);
  }

  map(token: string) {
    return this.call<MapView>("GET", "/map", undefined, token);
  }

  setFlag(token: string, mediaId: string, flag: "pin" | "hide", value: boolean) {
    return this.call<{ pinned: boolean; hidden: boolean }>("POST", `/media/${mediaId}/${flag}`, { value }, token);
  }
}
