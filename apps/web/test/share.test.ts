import { describe, expect, it } from "vitest";
import { Api, ApiError } from "../src/api.js";
import { closedMessage, shareLine } from "../src/pages/share.js";

describe("shared album page", () => {
  it("says who shared it, how many photos and how long it lasts", () => {
    const album = { name: "Kyoto temples", shared_by: "Hanyu's living room", expires_at: "2026-09-16T10:00:00Z", items: [{ id: "m1", url: "u", taken_at: null, landmark: null }] };
    expect(shareLine(album, new Date("2026-09-09T10:00:00Z"))).toBe("Shared by Hanyu's living room · 1 photo · 7 days left");
    expect(shareLine({ ...album, shared_by: null, items: [] }, new Date("2026-09-16T01:00:00Z"))).toBe("0 photos · expires today");
  });

  it("explains an expired or unknown link", () => {
    expect(closedMessage(new ApiError(410, "this link has expired"))).toMatch(/expired/);
    expect(closedMessage(new ApiError(404, "no such share"))).toMatch(/does not open/);
    expect(closedMessage(new Error("offline"))).toBe("offline");
  });

  it("opens the album without any session", async () => {
    const seen: Array<{ url: string; headers: Record<string, string> }> = [];
    const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({ url: String(input), headers: (init?.headers as Record<string, string>) ?? {} });
      return new Response(JSON.stringify({ name: "Kyoto temples", shared_by: null, expires_at: null, items: [] }), { status: 200 });
    }) as typeof fetch;
    const album = await new Api("https://api.test", fetchFn).sharedAlbum("abc 1");
    expect(album.name).toBe("Kyoto temples");
    expect(seen[0]!.url).toBe("https://api.test/s/abc%201");
    expect(seen[0]!.headers["authorization"]).toBeUndefined();
  });
});
