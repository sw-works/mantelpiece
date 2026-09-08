import { describe, expect, it } from "vitest";
import { Api, ApiError } from "../src/api.js";
import { DuplicateError, uploadFile } from "../src/upload/multipart.js";
import { runQueue } from "../src/upload/queue.js";
import { fingerprintOf, MemoryResumeStore } from "../src/upload/store.js";

interface Call {
  method: string;
  url: string;
  body?: unknown;
}

/** Fake backend: API JSON routes plus presigned PUTs, with programmable PUT failures. */
function fakeBackend(opts: { partSize?: number; failPuts?: number[]; duplicate?: boolean } = {}) {
  const calls: Call[] = [];
  const partSize = opts.partSize ?? 1024;
  const failPuts = new Set(opts.failPuts ?? []);
  let putCount = 0;
  let inits = 0;
  const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    const body = typeof init?.body === "string" ? JSON.parse(init.body) : init?.body;
    calls.push({ method, url, body });
    if (url.startsWith("https://s3.test/")) {
      putCount++;
      if (failPuts.has(putCount)) return new Response("slow down", { status: 503 });
      return new Response(null, { status: 200, headers: { etag: `"etag-${url.split("partNumber=")[1]}"` } });
    }
    if (url.endsWith("/uploads/init")) {
      inits++;
      if (opts.duplicate) return new Response(JSON.stringify({ error: "duplicate of m0" }), { status: 409 });
      const parts = Math.ceil((body as { bytes: number }).bytes / partSize);
      return new Response(
        JSON.stringify({
          media_id: `m${inits}`,
          upload_id: `u${inits}`,
          key: `originals/h/c/m${inits}.jpg`,
          part_size: partSize,
          part_urls: Array.from({ length: parts }, (_, i) => `https://s3.test/m${inits}?partNumber=${i + 1}`),
        }),
        { status: 201 },
      );
    }
    if (url.endsWith("/uploads/complete")) return new Response(JSON.stringify({ media_id: (body as { media_id: string }).media_id }), { status: 200 });
    return new Response(JSON.stringify({ error: "no route" }), { status: 404 });
  }) as typeof fetch;
  return { fetchFn, calls, api: new Api("https://api.test", fetchFn) };
}

function file(name: string, bytes: number): File {
  return new File([new Uint8Array(bytes).map((_, i) => i & 0xff)], name, { type: "image/jpeg", lastModified: 1_700_000_000_000 });
}

const noSleep = async () => undefined;

describe("uploadFile", () => {
  it("hashes, inits, puts every part, completes, clears the resume record", async () => {
    const b = fakeBackend();
    const store = new MemoryResumeStore();
    const phases: string[] = [];
    const f = file("a.jpg", 2500);
    const { mediaId } = await uploadFile(f, {
      api: b.api, bearer: "t", store, batchId: "b", batchTotal: 1, batchDone: 1, fetchFn: b.fetchFn, sleep: noSleep,
      onProgress: (p) => phases.push(p.phase),
    });
    expect(mediaId).toBe("m1");
    const puts = b.calls.filter((c) => c.method === "PUT");
    expect(puts).toHaveLength(3);
    expect((puts[2]!.body as Blob).size).toBe(2500 - 2048);
    const complete = b.calls.find((c) => c.url.endsWith("/uploads/complete"))!;
    expect(complete.body).toMatchObject({ media_id: "m1", upload_id: "u1", batch_done: 1, batch_total: 1 });
    expect((complete.body as { parts: unknown[] }).parts).toEqual([
      { part_number: 1, etag: '"etag-1"' },
      { part_number: 2, etag: '"etag-2"' },
      { part_number: 3, etag: '"etag-3"' },
    ]);
    expect(await store.get(fingerprintOf(f))).toBeUndefined();
    expect(phases[0]).toBe("hashing");
    expect(phases.at(-1)).toBe("done");
  });

  it("retries a failed part with backoff and reports the duplicate case", async () => {
    const b = fakeBackend({ failPuts: [2, 3] });
    const store = new MemoryResumeStore();
    const slept: number[] = [];
    await uploadFile(file("a.jpg", 2500), {
      api: b.api, bearer: "t", store, batchId: "b", batchTotal: 1, batchDone: 1, fetchFn: b.fetchFn,
      sleep: async (ms) => { slept.push(ms); },
    });
    expect(b.calls.filter((c) => c.method === "PUT")).toHaveLength(5);
    expect(slept).toEqual([500, 1000]);

    const dup = fakeBackend({ duplicate: true });
    await expect(
      uploadFile(file("a.jpg", 10), { api: dup.api, bearer: "t", store, batchId: "b", batchTotal: 1, batchDone: 1, fetchFn: dup.fetchFn, sleep: noSleep }),
    ).rejects.toBeInstanceOf(DuplicateError);
  });

  it("resumes from the store without re-hashing or re-uploading finished parts", async () => {
    const b = fakeBackend();
    const store = new MemoryResumeStore();
    const f = file("big.mov", 3000);
    await store.put({
      fingerprint: fingerprintOf(f), mediaId: "m9", uploadId: "u9", key: "k", partSize: 1024,
      partUrls: [1, 2, 3].map((n) => `https://s3.test/m9?partNumber=${n}`),
      completed: [{ part_number: 1, etag: '"old-1"' }, { part_number: 2, etag: '"old-2"' }],
      sha256: "x", batchId: "b", createdAt: 0,
    });
    const { mediaId } = await uploadFile(f, { api: b.api, bearer: "t", store, batchId: "b", batchTotal: 1, batchDone: 1, fetchFn: b.fetchFn, sleep: noSleep });
    expect(mediaId).toBe("m9");
    expect(b.calls.some((c) => c.url.endsWith("/uploads/init"))).toBe(false);
    expect(b.calls.filter((c) => c.method === "PUT").map((c) => c.url)).toEqual(["https://s3.test/m9?partNumber=3"]);
    const complete = b.calls.find((c) => c.url.endsWith("/uploads/complete"))!;
    expect((complete.body as { parts: { part_number: number }[] }).parts.map((p) => p.part_number)).toEqual([1, 2, 3]);
  });

  it("gives up on a 4xx part rejection", async () => {
    const rejecting = (async (input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input).startsWith("https://s3.test/")) return new Response("nope", { status: 403 });
      return fakeBackend().fetchFn(input, init);
    }) as typeof fetch;
    const api = new Api("https://api.test", rejecting);
    await expect(
      uploadFile(file("a.jpg", 10), { api, bearer: "t", store: new MemoryResumeStore(), batchId: "b", batchTotal: 1, batchDone: 1, fetchFn: rejecting, sleep: noSleep }),
    ).rejects.toThrow("rejected: 403");
  });
});

describe("runQueue", () => {
  it("uploads with bounded concurrency and keeps failures in the list", async () => {
    const b = fakeBackend({ duplicate: false });
    let active = 0;
    let peak = 0;
    const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
      if (String(input).startsWith("https://s3.test/")) {
        active++;
        peak = Math.max(peak, active);
        await new Promise((r) => setTimeout(r, 2));
        active--;
      }
      return b.fetchFn(input, init);
    }) as typeof fetch;
    const api = new Api("https://api.test", fetchFn);
    const files = [file("1.jpg", 100), file("2.jpg", 100), file("3.jpg", 100), file("4.jpg", 100)];
    const changes: number[] = [];
    const items = await runQueue(files, { api, bearer: "t", store: new MemoryResumeStore(), concurrency: 2, fetchFn, sleep: noSleep, onChange: (i) => changes.push(i.length) });
    expect(items.map((i) => i.mediaId).sort()).toEqual(["m1", "m2", "m3", "m4"]);
    expect(peak).toBeLessThanOrEqual(2);
    expect(changes.length).toBeGreaterThan(4);
    const totals = b.calls.filter((c) => c.url.endsWith("/uploads/complete")).map((c) => (c.body as { batch_total: number; batch_done: number }));
    expect(totals.every((t) => t.batch_total === 4)).toBe(true);
    expect(totals.map((t) => t.batch_done).sort()).toEqual([1, 2, 3, 4]);
  });

  it("marks duplicates without stopping the batch", async () => {
    const dup = fakeBackend({ duplicate: true });
    const items = await runQueue([file("a.jpg", 10)], { api: dup.api, bearer: "t", store: new MemoryResumeStore(), fetchFn: dup.fetchFn, sleep: noSleep });
    expect(items[0]!.progress.phase).toBe("duplicate");
    expect(items[0]!.error).toContain("duplicate of m0");
  });
});

describe("Api", () => {
  it("surfaces backend errors with status", async () => {
    const api = new Api("https://api.test", (async () => new Response(JSON.stringify({ error: "gone" }), { status: 410 })) as typeof fetch);
    await expect(api.joinAsGuest("X", "Maya")).rejects.toMatchObject({ status: 410, message: "gone" } satisfies Partial<ApiError>);
  });
});
