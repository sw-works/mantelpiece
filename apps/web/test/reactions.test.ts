import { describe, expect, it } from "vitest";
import { Api } from "../src/api.js";
import { latestComment, reactionLine, targetKey, viewerName } from "../src/reactions.js";

class MemStorage implements Storage {
  private m = new Map<string, string>();
  get length() {
    return this.m.size;
  }
  clear() {
    this.m.clear();
  }
  getItem(k: string) {
    return this.m.get(k) ?? null;
  }
  key(i: number) {
    return [...this.m.keys()][i] ?? null;
  }
  removeItem(k: string) {
    this.m.delete(k);
  }
  setItem(k: string, v: string) {
    this.m.set(k, v);
  }
}

describe("reactions on the read-only page", () => {
  const view = {
    reactions: [],
    summary: {
      "day:3": { likes: 2, comments: 1, latest_comment: { by: "Grandma Ruth", text: "That ramen!" } },
      "media:m1": { likes: 1, comments: 0, latest_comment: null },
    },
  };

  it("keys targets and summarises counts and the latest comment", () => {
    expect(targetKey({ target: "day", index: 3 })).toBe("day:3");
    expect(targetKey({ target: "media", id: "m1" })).toBe("media:m1");
    expect(reactionLine(view, "day:3")).toBe("❤ 2 · 1 comment");
    expect(reactionLine(view, "media:m1")).toBe("❤ 1");
    expect(reactionLine(view, "day:9")).toBe("");
    expect(reactionLine(null, "day:3")).toBe("");
    expect(latestComment(view, "day:3")).toBe("Grandma Ruth: That ramen!");
    expect(latestComment(view, "media:m1")).toBeNull();
  });

  it("keeps the viewer's name on the phone and sends it with each reaction", async () => {
    const storage = new MemStorage();
    expect(viewerName(storage)).toBeNull();
    storage.setItem("ml.viewer.name", " Grandma Ruth ");
    expect(viewerName(storage)).toBe("Grandma Ruth");
    const seen: Array<{ url: string; body: unknown }> = [];
    const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({ url: String(input), body: init?.body ? JSON.parse(String(init.body)) : undefined });
      return new Response(JSON.stringify({ reactions: [], summary: {} }), { status: 200 });
    }) as typeof fetch;
    const api = new Api("https://api.test", fetchFn);
    await api.react("T2C3D4", "kyoto", { target: "day", index: 3, kind: "comment", text: "That ramen!", name: "Grandma Ruth" });
    await api.reactions("T2C3D4", "kyoto");
    expect(seen[0]!.url).toBe("https://api.test/trips/kyoto/reactions?code=T2C3D4");
    expect(seen[0]!.body).toEqual({ target: "day", index: 3, kind: "comment", text: "That ramen!", name: "Grandma Ruth" });
    expect(seen[1]!.url).toBe("https://api.test/trips/kyoto/reactions?code=T2C3D4");
  });
});
