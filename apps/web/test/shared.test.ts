import { describe, expect, it } from "vitest";
import { sharedCount, takeSharedFiles, type SharedCache } from "../src/upload/shared.js";

/** In-memory stand-in for the Cache API entries the service worker writes. */
function fakeCache(entries: Array<[string, Response]>): SharedCache & { size: () => number } {
  const map = new Map(entries);
  return {
    keys: async () => [...map.keys()].map((url) => ({ url })),
    match: async (url) => map.get(url),
    delete: async (url) => map.delete(url),
    size: () => map.size,
  };
}

function parked(name: string, body: string, type = "image/jpeg", modified = 1_700_000_000_000): Response {
  return new Response(body, {
    headers: { "content-type": type, "x-file-name": encodeURIComponent(name), "x-file-modified": String(modified) },
  });
}

describe("share target", () => {
  it("reads the shared count only from a share-sheet launch", () => {
    expect(sharedCount(new URL("https://upload.test/?shared=3"))).toBe(3);
    expect(sharedCount(new URL("https://upload.test/?t=ABC123"))).toBe(0);
    expect(sharedCount(new URL("https://upload.test/?shared=nope"))).toBe(0);
    expect(sharedCount(new URL("https://upload.test/?shared=-2"))).toBe(0);
  });

  it("turns parked responses back into files, oldest first, and empties the cache", async () => {
    const cache = fakeCache([
      ["https://upload.test/shared/1700-1", parked("IMG_0002.jpg", "second")],
      ["https://upload.test/shared/1700-0", parked("Loch Ness & co.HEIC", "first", "image/heic", 42)],
    ]);
    const files = await takeSharedFiles(cache);
    expect(files.map((f) => f.name)).toEqual(["Loch Ness & co.HEIC", "IMG_0002.jpg"]);
    expect(files[0]!.type).toBe("image/heic");
    expect(files[0]!.lastModified).toBe(42);
    expect(await files[1]!.text()).toBe("second");
    expect(cache.size()).toBe(0);
  });

  it("names a file the share sheet left unnamed and copes with no cache at all", async () => {
    const cache = fakeCache([["https://upload.test/shared/1-0", new Response("x", { headers: { "content-type": "image/png" } })]]);
    const files = await takeSharedFiles(cache);
    expect(files.map((f) => f.name)).toEqual(["shared-1"]);
    expect(await takeSharedFiles(undefined)).toEqual([]);
  });
});
