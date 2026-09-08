// One file: init (or resume), PUT each part to its presigned URL with retries, complete.

import { type Api, type CompletedPart } from "../api.js";
import { sha256Blob } from "../sha256.js";
import { fingerprintOf, type ResumeRecord, type ResumeStore } from "./store.js";

export interface UploadProgress {
  phase: "hashing" | "uploading" | "done" | "duplicate" | "failed";
  fraction: number;
  message?: string;
}

export interface UploadOptions {
  api: Api;
  bearer: string;
  store: ResumeStore;
  batchId: string;
  batchTotal: number;
  /** 1-based index of this file in the batch, reported on completion. */
  batchDone: number;
  fetchFn?: typeof fetch;
  onProgress?: (p: UploadProgress) => void;
  maxAttempts?: number;
  /** Test hook; defaults to exponential backoff. */
  sleep?: (ms: number) => Promise<void>;
}

export class DuplicateError extends Error {}

export async function uploadFile(file: File, opts: UploadOptions): Promise<{ mediaId: string }> {
  const fetchFn = opts.fetchFn ?? fetch;
  const sleep = opts.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)));
  const maxAttempts = opts.maxAttempts ?? 5;
  const progress = (p: UploadProgress) => opts.onProgress?.(p);
  const fingerprint = fingerprintOf(file);

  let record = await opts.store.get(fingerprint);
  if (!record) {
    progress({ phase: "hashing", fraction: 0 });
    const sha256 = await sha256Blob(file, (f) => progress({ phase: "hashing", fraction: f }));
    let init;
    try {
      init = await opts.api.initUpload(opts.bearer, {
        filename: file.name,
        bytes: file.size,
        sha256,
        content_type: file.type || "application/octet-stream",
        batch_id: opts.batchId,
      });
    } catch (e) {
      if (e instanceof Error && "status" in e && (e as { status: number }).status === 409) {
        progress({ phase: "duplicate", fraction: 1, message: e.message });
        throw new DuplicateError(e.message);
      }
      throw e;
    }
    record = {
      fingerprint,
      mediaId: init.media_id,
      uploadId: init.upload_id,
      key: init.key,
      partSize: init.part_size,
      partUrls: init.part_urls,
      completed: [],
      sha256,
      batchId: opts.batchId,
      createdAt: Date.now(),
    };
    await opts.store.put(record);
  }

  const total = record.partUrls.length;
  const done = new Set(record.completed.map((p) => p.part_number));
  for (let n = 1; n <= total; n++) {
    if (done.has(n)) continue;
    const url = record.partUrls[n - 1]!;
    const chunk = file.slice((n - 1) * record.partSize, Math.min(n * record.partSize, file.size));
    const etag = await putWithRetry(fetchFn, url, chunk, maxAttempts, sleep);
    record.completed.push({ part_number: n, etag });
    await opts.store.put(record);
    progress({ phase: "uploading", fraction: record.completed.length / total });
  }

  const parts: CompletedPart[] = [...record.completed].sort((a, b) => a.part_number - b.part_number);
  await opts.api.completeUpload(opts.bearer, {
    media_id: record.mediaId,
    upload_id: record.uploadId,
    parts,
    batch_done: opts.batchDone,
    batch_total: opts.batchTotal,
  });
  await opts.store.delete(fingerprint);
  progress({ phase: "done", fraction: 1 });
  return { mediaId: record.mediaId };
}

async function putWithRetry(
  fetchFn: typeof fetch,
  url: string,
  body: Blob,
  maxAttempts: number,
  sleep: (ms: number) => Promise<void>,
): Promise<string> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      const res = await fetchFn(url, { method: "PUT", body });
      if (res.ok) {
        const etag = res.headers.get("etag");
        if (!etag) throw new Error("S3 did not return an ETag; check the bucket CORS ExposeHeaders");
        return etag;
      }
      if (res.status >= 400 && res.status < 500 && res.status !== 429) {
        throw new Error(`part upload rejected: ${res.status}`);
      }
      lastError = new Error(`part upload failed: ${res.status}`);
    } catch (e) {
      if (e instanceof Error && e.message.startsWith("part upload rejected")) throw e;
      if (e instanceof Error && e.message.includes("ETag")) throw e;
      lastError = e;
    }
    await sleep(Math.min(30_000, 500 * 2 ** (attempt - 1)));
  }
  throw lastError instanceof Error ? lastError : new Error("part upload failed");
}

export type { ResumeRecord };
