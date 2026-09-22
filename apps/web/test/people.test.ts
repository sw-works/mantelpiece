import { describe, expect, it } from "vitest";
import { Api } from "../src/api.js";
import { CONSENT, mergeTargets, personLabel, sortPeople } from "../src/people.js";

const people = [
  { id: "p1", name: null, media_count: 3, sample_media: [], faces: 3 },
  { id: "p2", name: "Maya", media_count: 12, sample_media: [], faces: 9 },
  { id: "p3", name: null, media_count: 7, sample_media: [], faces: 2 },
];

describe("People page", () => {
  it("labels and orders clusters, named first", () => {
    expect(personLabel(people[1]!)).toBe("Maya · 12 photos");
    expect(personLabel({ ...people[0]!, media_count: 1 })).toBe("Unnamed · 1 photo");
    expect(sortPeople(people).map((p) => p.id)).toEqual(["p2", "p3", "p1"]);
    expect(mergeTargets(people, "p3").map((p) => p.id)).toEqual(["p2", "p1"]);
    expect(CONSENT.on).toMatch(/deleted the moment it is turned off on the TV/);
  });

  it("names and merges through the API", async () => {
    const seen: Array<{ method: string; url: string; body: unknown }> = [];
    const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({ method: init?.method ?? "GET", url: String(input), body: init?.body ? JSON.parse(String(init.body)) : undefined });
      return new Response(JSON.stringify({ people_enabled: true, people: [] }), { status: 200 });
    }) as typeof fetch;
    const api = new Api("https://api.test", fetchFn);
    await api.updatePerson("t", "p1", { name: "Maya" });
    await api.updatePerson("t", "p3", { merge_into: "p2" });
    await api.people("t");
    expect(seen.map((s) => `${s.method} ${s.url}`)).toEqual([
      "PATCH https://api.test/people/p1",
      "PATCH https://api.test/people/p3",
      "GET https://api.test/people",
    ]);
    expect(seen[1]!.body).toEqual({ merge_into: "p2" });
  });
});
