import { describe, expect, it } from "vitest";
import { Api } from "../src/api.js";
import { castConfirmation, groupByYear, tripSummaryLine, untilConfirmed } from "../src/trips.js";

describe("my trips", () => {
  it("groups by start year, newest first, and keeps the API order inside a year", () => {
    const groups = groupByYear([
      { id: "kyoto", start: "2026-04-03T00:00:00Z" },
      { id: "rome", start: "2025-09-01T00:00:00Z" },
      { id: "skye", start: "2025-04-04T00:00:00Z" },
    ]);
    expect(groups.map((g) => [g.year, g.trips.map((t) => t.id)])).toEqual([
      ["2026", ["kyoto"]],
      ["2025", ["rome", "skye"]],
    ]);
  });

  it("summarises dates, length and highlights", () => {
    expect(tripSummaryLine({ start: "2026-04-03T00:00:00Z", end: "2026-04-09T00:00:00Z", highlights: ["m1", "m2"] })).toBe("2026-04-03 to 2026-04-09 · 7 days · 2 highlights");
    expect(tripSummaryLine({ start: "2026-04-03T10:00:00Z", end: "2026-04-03T18:00:00Z", highlights: [] })).toBe("2026-04-03 · 1 day");
  });

  it("asks the TV to play a trip with the owner session", async () => {
    const seen: Array<{ url: string; method: string | undefined; body: unknown }> = [];
    const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({ url: String(input), method: init?.method, body: typeof init?.body === "string" ? JSON.parse(init.body) : init?.body });
      return new Response(JSON.stringify({ requested: true, trip: "kyoto", name: "Kyoto" }), { status: 200 });
    }) as typeof fetch;
    const out = await new Api("https://api.test", fetchFn).playTrip("owner-token", "kyoto");
    expect(out.requested).toBe(true);
    expect(seen).toEqual([{ url: "https://api.test/trips/kyoto/play", method: "POST", body: { source: "phone" } }]);
  });

  it("casts one photo: the play request carries the media id to start at", async () => {
    const seen: Array<{ url: string; body: unknown }> = [];
    const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({ url: String(input), body: typeof init?.body === "string" ? JSON.parse(init.body) : init?.body });
      return new Response(JSON.stringify({ requested: true, trip: "kyoto", name: "Kyoto", start: "m7" }), { status: 200 });
    }) as typeof fetch;
    const out = await new Api("https://api.test", fetchFn).playTrip("owner-token", "kyoto", "m7");
    expect(out.start).toBe("m7");
    expect(seen).toEqual([{ url: "https://api.test/trips/kyoto/play", body: { source: "phone", start: "m7" } }]);
  });

  it("confirms the cast from presence once a TV shows the trip at the photo", () => {
    const before = [
      { device_id: "tv-1", media: "old", trip: "rome", day: 2, paused: false, at: "2026-09-15T10:00:00Z" },
      { device_id: "tv-2", media: "m7", trip: "kyoto", day: 1, paused: false, at: "2026-09-15T09:00:00Z" },
    ];
    // Nothing changed yet: tv-2 was already on m7 before the request, so it does not count.
    expect(castConfirmation(before, before, { trip: "kyoto", start: "m7" })).toBeNull();
    const after = [
      { device_id: "tv-1", media: "m7", trip: "kyoto", day: 3, paused: false, at: "2026-09-15T10:00:05Z" },
      { device_id: "tv-2", media: "m7", trip: "kyoto", day: 1, paused: false, at: "2026-09-15T09:00:00Z" },
    ];
    expect(castConfirmation(after, before, { trip: "kyoto", start: "m7" })).toBe("Now showing on your TV, day 3.");
    // Whole-trip cast: any photo of the trip counts, and a TV that was not around before counts too.
    const two = [...after, { device_id: "tv-3", media: "m1", trip: "kyoto", day: 1, paused: false, at: "2026-09-15T10:00:06Z" }];
    expect(castConfirmation(two, before, { trip: "kyoto" })).toBe("Now showing on 2 TVs, day 3.");
    // Wrong photo does not confirm a photo cast.
    expect(castConfirmation([{ device_id: "tv-1", media: "m1", trip: "kyoto", day: 1, paused: false, at: "2026-09-15T10:00:07Z" }], before, { trip: "kyoto", start: "m7" })).toBeNull();
  });

  it("polls until confirmed, then stops; gives up after the tries", async () => {
    let calls = 0;
    const line = await untilConfirmed(async () => (++calls >= 3 ? "Now showing on your TV." : null), 6, 0, async () => undefined);
    expect([line, calls]).toEqual(["Now showing on your TV.", 3]);
    calls = 0;
    const none = await untilConfirmed(async () => { calls++; throw new Error("offline"); }, 2, 0, async () => undefined);
    expect([none, calls]).toEqual([null, 2]);
  });

  it("fetches one trip with the owner session", async () => {
    const seen: Array<{ url: string; auth: string | undefined }> = [];
    const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({ url: String(input), auth: (init?.headers as Record<string, string>)?.authorization });
      return new Response(JSON.stringify({ id: "kyoto", name: "Kyoto", days: [] }), { status: 200 });
    }) as typeof fetch;
    const trip = await new Api("https://api.test", fetchFn).trip("owner-token", "kyoto");
    expect(trip.name).toBe("Kyoto");
    expect(seen).toEqual([{ url: "https://api.test/trips/kyoto", auth: "Bearer owner-token" }]);
  });
});
