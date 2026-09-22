import { type DevicePoll, type FeedEvent, type Trip, type TripSummary, TvApi } from "../src/api/client";

/** Scripted backend for hook and screen tests. */
export class FakeApi extends TvApi {
  polls: DevicePoll[] = [];
  eventPages: Array<{ events: FeedEvent[]; next: number }> = [];
  tripList: TripSummary[] = [];
  tripDetail: Trip | null = null;
  calls: string[] = [];

  constructor() {
    super("https://fake", () => Promise.reject(new Error("no network in tests")));
  }

  /** Ten minutes from now, like the server. */
  expiresInMs = 600_000;

  override async issueDeviceCode(deviceId: string, tvName?: string) {
    this.calls.push(`issue ${deviceId}${tvName ? ` ${tvName}` : ""}`);
    return { code: "AB2C3D4E", link: "https://link.test/?c=AB2C3D4E", expires_at: new Date(Date.now() + this.expiresInMs).toISOString(), poll_seconds: 25 };
  }
  override async pollDeviceCode(code: string) {
    this.calls.push(`poll ${code}`);
    return this.polls.length > 1 ? this.polls.shift()! : (this.polls[0] ?? { state: "pending" });
  }
  override async events(_token: string, since: number) {
    this.calls.push(`events ${since}`);
    if (this.eventPages.length === 0) {
      await new Promise((r) => setTimeout(r, 50));
      return { events: [], next: since };
    }
    return this.eventPages.shift()!;
  }
  override async trips() {
    this.calls.push("trips");
    return this.tripList;
  }
  override async trip(_token: string, id: string) {
    this.calls.push(`trip ${id}`);
    if (!this.tripDetail) throw new Error("no trip");
    return this.tripDetail;
  }
  override async mediaUrl(_token: string, mediaId: string) {
    return { url: `https://cdn.test/${mediaId}.jpg`, expires_in: 3600 };
  }
  override async tripMap(_token: string, tripId: string, day?: number) {
    this.calls.push(`map ${tripId}${day === undefined ? "" : ` day ${day}`}`);
    return { url: `https://cdn.test/map-${tripId}-${day ?? "trip"}.png`, inferred: day === 2 };
  }
  override async regenerateNarrative(_token: string, tripId: string) {
    this.calls.push(`regenerate ${tripId}`);
    return { requested: true };
  }
  searchResults: Array<{ media: string; score: number; trip: string | null; contributor: string }> = [];
  override async search(_token: string, query: string) {
    this.calls.push(`search ${query}`);
    return { query, results: this.searchResults };
  }
  override async putPresence(_token: string, p: { device_id: string; media: string; paused: boolean }) {
    this.calls.push(`presence ${p.device_id} ${p.media} ${p.paused ? "paused" : "playing"}`);
  }
  override async previewTrip(code: string) {
    this.calls.push(`preview ${code}`);
    if (code === "BADBAD") throw new Error("unknown code");
    return { household_name: "Hanyu's living room", trip: { ...(this.tripDetail as Trip), id: "kyoto", name: "Kyoto, April 2026" } };
  }
  override async joinTrip(_token: string, code: string) {
    this.calls.push(`join ${code}`);
    return { joined: true, trip: "kyoto", owner: "amzn-owner", name: "Kyoto, April 2026" };
  }
  override async leaveTrip(_token: string, tripId: string) {
    this.calls.push(`leave ${tripId}`);
  }
  override async setRole(_token: string, tripId: string, contributor: string, role: "co_editor" | "contributor") {
    this.calls.push(`role ${tripId} ${contributor} ${role}`);
    return { contributor, role };
  }
  memories: import("../src/api/client").MemoriesHomeView | null = null;
  override async memoriesHome() {
    this.calls.push("memories");
    if (!this.memories) throw new Error("no memories");
    return this.memories;
  }
  mapData: import("../src/api/client").MapView = { url: null, pins: [] };
  override async map() {
    this.calls.push("map");
    return this.mapData;
  }
  peopleState: { people_enabled: boolean; people: import("../src/api/client").PersonView[] } = { people_enabled: false, people: [] };
  override async people() {
    this.calls.push("people");
    return this.peopleState;
  }
  override async setPeople(_token: string, enabled: boolean) {
    this.calls.push(`people ${enabled ? "on" : "off"}`);
    this.peopleState = { people_enabled: enabled, people: [] };
    return { people_enabled: enabled };
  }
  reactionsView: import("../src/api/client").ReactionsView = { reactions: [], summary: {} };
  override async reactions(_token: string, tripId: string) {
    this.calls.push(`reactions ${tripId}`);
    return this.reactionsView;
  }
  playlistList: import("../src/api/client").Playlist[] = [];
  override async playlists() {
    this.calls.push("playlists");
    return this.playlistList;
  }
  override async playlist(_token: string, id: string) {
    this.calls.push(`playlist ${id}`);
    return this.playlistList.find((p) => p.id === id)!;
  }
  override async narrationUrl(_token: string, tripId: string, day: number) {
    this.calls.push(`narration ${tripId} ${day}`);
    return { url: `https://cdn.test/${tripId}-${day}.mp3`, day };
  }
  override async setKeeper(_token: string, mediaId: string) {
    this.calls.push(`keeper ${mediaId}`);
    return { keeper: mediaId, changed: true };
  }
  override async householdCode() {
    this.calls.push("householdCode");
    return { code: "H2C3D4", scope: "household" as const, trip: null, link: "https://upload.test/?h=H2C3D4", accepts_uploads: true };
  }
  override async tripCode(_token: string, tripId: string) {
    this.calls.push(`tripCode ${tripId}`);
    return { code: "T2C3D4", scope: "trip" as const, trip: tripId, link: "https://upload.test/?t=T2C3D4", accepts_uploads: true, contributors: 3, member_households: 1 };
  }
  override async rotateCode(_token: string, code: string) {
    this.calls.push(`rotate ${code}`);
    return { code: "R2C3D4", scope: "trip" as const, trip: "kyoto", link: "https://upload.test/?t=R2C3D4", accepts_uploads: true, contributors: 3, member_households: 1 };
  }
  override async mintConfirmation(_token: string, action: Record<string, unknown>, proposal?: string) {
    this.calls.push(`confirm ${String(action.action)}${proposal ? ` ${proposal}` : ""}`);
    return { token: "conf-1", tool: String(action.action), expires_at: 0, proposal: proposal ?? null };
  }
  proposalList: import("../src/api/client").ProposalView[] = [];
  override async proposals() {
    this.calls.push("proposals");
    return this.proposalList;
  }
  override async cancelProposal(_token: string, id: string) {
    this.calls.push(`cancel ${id}`);
    const p = this.proposalList.find((x) => x.id === id)!;
    return { ...p, state: "cancelled" as const };
  }
  pairing: import("../src/api/client").PairQueue = { pending: [], phones: [] };
  override async setupTv(deviceId: string, name: string) {
    this.calls.push(`setup ${deviceId} ${name}`);
    return { household: "tv-house", household_name: name, token: "tv-token" };
  }
  override async pairCode() {
    this.calls.push("pair code");
    return { code: "PA1R2C0D", link: "https://link.test/?p=PA1R2C0D", expires_at: new Date(Date.now() + this.expiresInMs).toISOString() };
  }
  override async pairQueue() {
    this.calls.push("pair queue");
    return this.pairing;
  }
  override async decidePhone(_token: string, id: string, allow: boolean) {
    this.calls.push(`${allow ? "allow" : "deny"} ${id}`);
    const phone = [...this.pairing.pending, ...this.pairing.phones].find((p) => p.id === id)!;
    this.pairing = {
      pending: this.pairing.pending.filter((p) => p.id !== id),
      phones: allow ? [...this.pairing.phones, phone] : this.pairing.phones.filter((p) => p.id !== id),
    };
    return { id, name: phone.name, approved: allow };
  }
  loveError: Error | null = null;
  override async loveArrival(_token: string, body: { contributor: string; count: number; cover?: string }) {
    this.calls.push(`love ${body.contributor} ${body.count}${body.cover ? ` ${body.cover}` : ""}`);
    if (this.loveError) throw this.loveError;
    return { pushed: true, noted: true };
  }
  override async stopLive() {
    this.calls.push("stop live");
    return { ended: true };
  }
  override async grantLive(_token: string, request: string) {
    this.calls.push(`grant ${request}`);
    return { broadcast: `b-${request}` };
  }
  override async declineLive(_token: string, request: string) {
    this.calls.push(`decline ${request}`);
  }
}
