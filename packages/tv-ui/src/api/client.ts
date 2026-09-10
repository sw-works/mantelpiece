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

  private async call<T>(method: "GET" | "POST" | "DELETE", path: string, body?: unknown, bearer?: string): Promise<T> {
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

  rotateCode(token: string, code: string) {
    return this.call<CodeView>("POST", `/codes/${code}/rotate`, undefined, token);
  }

  mintConfirmation(token: string, action: Record<string, unknown>) {
    return this.call<{ token: string; tool: string; expires_at: number }>("POST", "/confirmations", action, token);
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

  setFlag(token: string, mediaId: string, flag: "pin" | "hide", value: boolean) {
    return this.call<{ pinned: boolean; hidden: boolean }>("POST", `/media/${mediaId}/${flag}`, { value }, token);
  }
}
