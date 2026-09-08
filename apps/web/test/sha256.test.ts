import { createHash } from "node:crypto";
import { describe, expect, it } from "vitest";
import { Sha256, sha256Blob } from "../src/sha256.js";

describe("Sha256", () => {
  it("matches known vectors", () => {
    expect(new Sha256().hex()).toBe("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    expect(new Sha256().update(new TextEncoder().encode("abc")).hex()).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });

  it("matches node across chunk boundaries and sizes", () => {
    for (const size of [55, 56, 63, 64, 65, 119, 120, 1000, 70_000]) {
      const data = new Uint8Array(size).map((_, i) => (i * 31 + 7) & 0xff);
      const expected = createHash("sha256").update(data).digest("hex");
      const h = new Sha256();
      for (let i = 0; i < size; i += 13) h.update(data.subarray(i, Math.min(i + 13, size)));
      expect(h.hex()).toBe(expected);
    }
  });

  it("hashes blobs in slices and reports progress", async () => {
    const data = new Uint8Array(100_000).map((_, i) => i & 0xff);
    const expected = createHash("sha256").update(data).digest("hex");
    const seen: number[] = [];
    expect(await sha256Blob(new Blob([data]), (f) => seen.push(f), 30_000)).toBe(expected);
    expect(seen.at(-1)).toBe(1);
    expect(seen.length).toBeGreaterThan(2);
  });
});
