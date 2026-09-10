import { describe, expect, it } from "vitest";
import { Api } from "../src/api.js";
import { tripLines } from "../src/pages/trip.js";

describe("read-only trip page", () => {
  it("lists days with dates, counts and credits", () => {
    const lines = tripLines({
      id: "kyoto",
      name: "Kyoto",
      start: "2026-04-03T00:00:00Z",
      end: "2026-04-05T00:00:00Z",
      hero: null,
      highlights: [],
      narrative: null,
      contributor_names: { owner: "Hanyu", maya: "Maya" },
      media_credits: { m1: "owner", m2: "maya" },
      days: [
        { index: 1, date: "2026-04-03", media: ["m1", "m2"], title: "Day 1: Kinkaku-ji", route: { inferred: false } },
        { index: 2, date: "2026-04-04", media: ["m3"], title: null, route: { inferred: true } },
      ],
    });
    expect(lines).toEqual(["Day 1: Kinkaku-ji · 2026-04-03 · 2 photos by Hanyu, Maya", "Day 2 · 2026-04-04 · 1 photo · route inferred"]);
  });

  it("opens the trip by code without a session and binds guests to the device", async () => {
    const seen: Array<{ url: string; headers: Record<string, string> }> = [];
    const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({ url: String(input), headers: (init?.headers as Record<string, string>) ?? {} });
      return new Response(JSON.stringify({ household_name: "Living room", trip: { id: "kyoto" } }), { status: 200 });
    }) as typeof fetch;
    const api = new Api("https://api.test", fetchFn, "phone-a");
    const view = await api.viewTrip("T2 C3d4");
    expect(view.household_name).toBe("Living room");
    expect(seen[0]!.url).toBe("https://api.test/view?code=T2%20C3d4");
    expect(seen[0]!.headers["x-device-id"]).toBe("phone-a");
    expect(seen[0]!.headers["authorization"]).toBeUndefined();
    await api.joinAsGuest("T2C3D4", "Maya");
    expect(seen[1]!.url).toBe("https://api.test/join");
    await api.joinTrip("owner-token", "T2C3D4");
    expect(seen[2]!.url).toBe("https://api.test/memberships");
    expect(seen[2]!.headers["authorization"]).toBe("Bearer owner-token");
  });
});
