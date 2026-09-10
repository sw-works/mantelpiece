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

  override async issueDeviceCode(deviceId: string) {
    this.calls.push(`issue ${deviceId}`);
    return { code: "AB2C3D4E", link: "https://link.test/?c=AB2C3D4E", expires_at: "2026-09-08T10:10:00Z", poll_seconds: 25 };
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
  override async householdCode() {
    this.calls.push("householdCode");
    return { code: "H2C3D4", scope: "household" as const, trip: null, link: "https://upload.test/?h=H2C3D4", accepts_uploads: true };
  }
  override async tripCode(_token: string, tripId: string) {
    this.calls.push(`tripCode ${tripId}`);
    return { code: "T2C3D4", scope: "trip" as const, trip: tripId, link: "https://upload.test/?t=T2C3D4", accepts_uploads: true };
  }
  override async mintConfirmation(_token: string, action: Record<string, unknown>) {
    this.calls.push(`confirm ${String(action.action)}`);
    return { token: "conf-1", tool: String(action.action), expires_at: 0 };
  }
}
