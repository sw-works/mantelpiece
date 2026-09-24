// Thin client over the backend API. Bearer is an app session token (owner phone or
// upload session); `jwt` is the Cognito id token for the two JWT-guarded routes.

export interface CompletedPart {
  part_number: number;
  etag: string;
}

export interface InitiatedUpload {
  media_id: string;
  upload_id: string;
  key: string;
  part_size: number;
  part_urls: string[];
}

export interface Joined {
  token: string;
  household: string;
  contributor: string;
  display_name: string;
  trip: string | null;
}

export interface TripDay {
  index: number;
  date: string;
  media: string[];
  title: string | null;
  route: { inferred: boolean };
}

export interface PersonView {
  id: string;
  name: string | null;
  media_count: number;
  sample_media: string[];
  faces: number;
}

export type AskResult =
  | { mode: "describe"; tv: string; question: string; card: { answer: string; landmark: string | null; taken_by: string; when: string | null } }
  | { mode: "search"; question: string; playlist: string; count: number }
  | { mode: "nothing"; question: string; hint: string };

export type ReactionTarget = { target: "day"; index: number } | { target: "media"; id: string };
export type ReactionBody = ReactionTarget & { kind: "like" | "comment"; text?: string; name: string };

export interface ReactionsView {
  reactions: Array<{ id: string; target: "day" | "media"; index?: number; actor_name: string; kind: "like" | "comment"; text?: string; created_at: string }>;
  summary: Record<string, { likes: number; comments: number; latest_comment: { by: string; text: string } | null }>;
}

/** The read-only view of one trip (F-159). */
/** A shared album for someone outside the household (F-601). */
export interface SharedAlbum {
  name: string;
  shared_by: string | null;
  expires_at: string | null;
  items: Array<{ id: string; url: string; taken_at: string | null; landmark: string | null }>;
}

export interface TripView {
  id: string;
  name: string;
  start: string;
  end: string;
  days: TripDay[];
  highlights: string[];
  hero: string | null;
  narrative: { text: string } | null;
  contributor_names?: Record<string, string>;
  media_credits?: Record<string, string>;
}

/** What the TV said about this phone's photos (family channel). */
export interface InboxNote {
  at: string;
  from: string;
  kind: string;
  count: number;
  cover?: string;
}

/** This phone asked the TV to go live; the TV answers within `expires_at`. */
export interface LiveAsk {
  request: string;
  status: "waiting";
  expires_at: string;
  /** Who is live now: a grant ends their broadcast. */
  replacing: string | null;
  /** The broadcast will be recorded into memories; false when asked but the stack cannot. */
  saving: boolean;
  tv: string;
}

/** The TV's answer. A grant carries where to stream; the key never leaves this phone. */
export interface LiveAnswer {
  request: string;
  status: "waiting" | "granted" | "declined" | "expired" | "cancelled" | "ended";
  tv: string;
  broadcast?: string;
  ingest_endpoint?: string;
  stream_key?: string;
  playback_url?: string;
  /** On a grant: the channel records, and the video goes into memories after. */
  saving?: boolean;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export type PairingStatus =
  | { state: "pending" }
  | { state: "denied" }
  | { state: "approved"; token: string; household: string; tv_name: string | null; name: string | null };

export class Api {
  constructor(
    readonly base: string,
    private readonly fetchFn: typeof fetch = fetch,
    /** Browser install id; guest sessions are bound to it (F-153). */
    readonly deviceId?: string,
  ) {}

  private async call<T>(
    method: "GET" | "POST" | "PUT" | "PATCH" | "DELETE",
    path: string,
    body?: unknown,
    auth?: { bearer?: string; jwt?: string },
  ): Promise<T> {
    const headers: Record<string, string> = {};
    if (body !== undefined) headers["content-type"] = "application/json";
    if (auth?.bearer) headers["authorization"] = `Bearer ${auth.bearer}`;
    if (auth?.jwt) headers["authorization"] = `Bearer ${auth.jwt}`;
    if (this.deviceId) headers["x-device-id"] = this.deviceId;
    // Called unbound: Chrome rejects `fetch` invoked with an `Api` as `this`.
    const res = await this.fetchFn.call(undefined, `${this.base}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (res.status === 204) return undefined as T;
    const json = (await res.json().catch(() => ({}))) as { error?: string };
    if (!res.ok) throw new ApiError(res.status, json.error ?? `HTTP ${res.status}`);
    return json as T;
  }

  approveDeviceCode(code: string, jwt: string) {
    return this.call<{ approved: boolean; household: string; token: string }>("POST", "/device/approve", { code }, { jwt });
  }

  /** Which TV a pairing code belongs to, before the phone asks to join it. */
  describePairing(code: string) {
    return this.call<{ tv_name: string; household: string }>("GET", `/pair/code/${encodeURIComponent(code)}`);
  }

  /** Ask the TV to let this phone in, under the name its owner typed. */
  requestPairing(code: string, name: string) {
    return this.call<{ request: string; name: string; tv_name: string; token: string }>("POST", "/pair/requests", { code, name });
  }

  /** How the request stands. The API holds the call while it is pending. */
  pairingStatus(waitingToken: string) {
    return this.call<PairingStatus>("GET", "/pair/status", undefined, { bearer: waitingToken });
  }

  joinAsGuest(code: string, displayName: string) {
    return this.call<Joined>("POST", "/join", { code, display_name: displayName, device_id: this.deviceId });
  }

  /** Join a trip from another household by its code, for this phone's TV (F-411). */
  joinTrip(bearer: string, code: string) {
    return this.call<{ joined: boolean; trip: string; owner: string; name?: string; already?: boolean }>("POST", "/memberships", { code }, { bearer });
  }

  /** People (F-704): opt-in face groups, named and merged from the phone. */
  people(bearer: string) {
    return this.call<{ people_enabled: boolean; people: PersonView[] }>("GET", "/people", undefined, { bearer });
  }

  updatePerson(bearer: string, id: string, body: { name?: string; merge_into?: string }) {
    return this.call<PersonView>("PATCH", `/people/${id}`, body, { bearer });
  }

  myUploads(bearer: string) {
    return this.call<Array<{ media_id: string; filename: string; kind: string; state: string; uploaded_at: string; trip: string | null }>>("GET", "/uploads/mine", undefined, { bearer });
  }

  /** What the TVs are showing right now (owner session). */
  presence(bearer: string) {
    return this.call<Array<{ device_id: string; media: string; trip: string | null; day: number | null; paused: boolean; at: string }>>("GET", "/presence", undefined, { bearer });
  }

  ask(bearer: string, question: string, tv?: string) {
    return this.call<AskResult>("POST", "/ask", { question, tv }, { bearer });
  }

  /** The read-only trip page: no session, just the trip code. */
  /** Public, no session: the link itself is the key. 410 once it expires. */
  sharedAlbum(id: string) {
    return this.call<SharedAlbum>("GET", `/s/${encodeURIComponent(id)}`);
  }

  viewTrip(code: string) {
    return this.call<{ household_name: string | null; trip: TripView }>("GET", `/view?code=${encodeURIComponent(code)}`);
  }

  /** Reactions on the read-only page (F-710): by code, with the viewer's name. */
  reactions(code: string, tripId: string) {
    return this.call<ReactionsView>("GET", `/trips/${tripId}/reactions?code=${encodeURIComponent(code)}`);
  }

  react(code: string, tripId: string, body: ReactionBody) {
    return this.call<unknown>("POST", `/trips/${tripId}/reactions?code=${encodeURIComponent(code)}`, body);
  }

  mediaUrlByCode(code: string, mediaId: string, rendition: "thumb" | "tv") {
    return this.call<{ url: string }>("GET", `/media/${mediaId}/url?rendition=${rendition}&code=${encodeURIComponent(code)}`);
  }

  joinWithAmazon(code: string, jwt: string) {
    return this.call<Joined>("POST", "/join/amazon", { code }, { jwt });
  }

  pushKey() {
    return this.call<{ key: string }>("GET", "/push/key");
  }

  /** `subscription` is `PushSubscription.toJSON()`. */
  subscribePush(bearer: string, subscription: { endpoint?: string; keys?: Record<string, string> }) {
    return this.call<{ push: boolean }>("PUT", "/push/subscription", subscription, { bearer });
  }

  unsubscribePush(bearer: string) {
    return this.call<void>("DELETE", "/push/subscription", undefined, { bearer });
  }

  inbox(bearer: string) {
    return this.call<{ notes: InboxNote[]; push: boolean }>("GET", "/inbox", undefined, { bearer });
  }

  askLive(bearer: string, body: { save: boolean; utc_offset_minutes?: number } = { save: false }) {
    return this.call<LiveAsk>("POST", "/live", body, { bearer });
  }

  liveAnswer(bearer: string) {
    return this.call<LiveAnswer>("GET", "/live/request", undefined, { bearer });
  }

  cancelLive(bearer: string) {
    return this.call<void>("DELETE", "/live/request", undefined, { bearer });
  }

  stopLive(bearer: string) {
    return this.call<{ ended: boolean }>("POST", "/live/stop", undefined, { bearer });
  }

  /** The household's broadcast now, or null: the TV can end it without this phone. */
  currentLive(bearer: string) {
    return this.call<{ live: { id: string; display_name: string } | null }>("GET", "/live", undefined, { bearer });
  }

  me(bearer: string) {
    return this.call<{ role: string; household: string; display_name?: string | null; household_name?: string | null; trip?: string | null }>("GET", "/me", undefined, { bearer });
  }

  initUpload(bearer: string, file: { filename: string; bytes: number; sha256: string; content_type: string; batch_id: string }) {
    return this.call<InitiatedUpload>("POST", "/uploads/init", file, { bearer });
  }

  completeUpload(
    bearer: string,
    body: { media_id: string; upload_id: string; parts: CompletedPart[]; batch_done: number; batch_total: number },
  ) {
    return this.call<{ media_id: string }>("POST", "/uploads/complete", body, { bearer });
  }

  abortUpload(bearer: string, media_id: string, upload_id: string) {
    return this.call<void>("POST", "/uploads/abort", { media_id, upload_id }, { bearer });
  }

  deleteMedia(bearer: string, mediaId: string) {
    return this.call<void>("DELETE", `/media/${mediaId}`, undefined, { bearer });
  }

  trips(bearer: string) {
    return this.call<Array<{ id: string; name: string; start: string; end: string; highlights: string[] }>>("GET", "/trips", undefined, { bearer });
  }

  /** One trip with its days, for the owner's phone (My trips). */
  trip(bearer: string, id: string) {
    return this.call<TripView>("GET", `/trips/${id}`, undefined, { bearer });
  }

  /** Ask the household's TV to play a trip; the TV opens it from its feed. */
  /** Ask the household's TVs to play a trip, from `start` (a media id) when given. */
  playTrip(bearer: string, id: string, start?: string) {
    return this.call<{ requested: boolean; trip: string; name: string; start: string | null }>("POST", `/trips/${id}/play`, { source: "phone", ...(start ? { start } : {}) }, { bearer });
  }

  mediaUrl(bearer: string, mediaId: string, rendition: "thumb" | "tv") {
    return this.call<{ url: string }>("GET", `/media/${mediaId}/url?rendition=${rendition}`, undefined, { bearer });
  }
}
