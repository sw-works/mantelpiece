import { describe, expect, it } from "vitest";
import { Api } from "../src/api.js";
import { groupByYear, tripSummaryLine } from "../src/trips.js";

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
