// A batch of files with bounded concurrency. Files that fail stay in the list with their
// error; the batch counter still reaches total so the backend starts organising.

import { type Api } from "../api.js";
import { DuplicateError, uploadFile, type UploadProgress } from "./multipart.js";
import { type ResumeStore } from "./store.js";

export interface QueueItem {
  file: File;
  progress: UploadProgress;
  /** The batch this file belongs to, kept so a retry reports the same batch. */
  batchId: string;
  mediaId?: string;
  error?: string;
}

export interface QueueOptions {
  api: Api;
  bearer: string;
  store: ResumeStore;
  concurrency?: number;
  fetchFn?: typeof fetch;
  sleep?: (ms: number) => Promise<void>;
  onChange?: (items: readonly QueueItem[]) => void;
}

export function newBatchId(): string {
  return crypto.randomUUID();
}

/** One line per file for the list on the phone (US2). */
export function labelFor(item: QueueItem): string {
  const pct = Math.round(item.progress.fraction * 100);
  switch (item.progress.phase) {
    case "hashing":
      return `checking ${pct}%`;
    case "uploading":
      return `uploading ${pct}%`;
    case "done":
      return "done";
    case "duplicate":
      return "duplicate, skipped";
    case "failed":
      return `failed: ${item.error ?? "unknown error"}`;
  }
}

/**
 * Upload one failed file again. Finished parts are reused from the resume store, so a
 * retry after a bad network only sends what is missing.
 */
export async function retryItem(item: QueueItem, opts: QueueOptions, batch: { total: number; done: number }): Promise<QueueItem> {
  if (item.progress.phase !== "failed") return item;
  item.error = undefined;
  item.progress = { phase: "uploading", fraction: 0 };
  opts.onChange?.([item]);
  try {
    const { mediaId } = await uploadFile(item.file, {
      api: opts.api,
      bearer: opts.bearer,
      store: opts.store,
      batchId: item.batchId,
      batchTotal: batch.total,
      batchDone: batch.done,
      fetchFn: opts.fetchFn,
      sleep: opts.sleep,
      onProgress: (p) => {
        item.progress = p;
        opts.onChange?.([item]);
      },
    });
    item.mediaId = mediaId;
  } catch (e) {
    item.error = e instanceof Error ? e.message : String(e);
    item.progress = { phase: e instanceof DuplicateError ? "duplicate" : "failed", fraction: 1, message: item.error };
  }
  opts.onChange?.([item]);
  return item;
}

export async function runQueue(files: File[], opts: QueueOptions): Promise<QueueItem[]> {
  const batchId = newBatchId();
  const items: QueueItem[] = files.map((file) => ({ file, batchId, progress: { phase: "hashing", fraction: 0 } }));
  const concurrency = Math.max(1, opts.concurrency ?? 3);
  let next = 0;
  let finished = 0;
  const notify = () => opts.onChange?.(items);

  const worker = async () => {
    while (next < items.length) {
      const index = next++;
      const item = items[index]!;
      try {
        const { mediaId } = await uploadFile(item.file, {
          api: opts.api,
          bearer: opts.bearer,
          store: opts.store,
          batchId,
          batchTotal: items.length,
          batchDone: ++finished,
          fetchFn: opts.fetchFn,
          sleep: opts.sleep,
          onProgress: (p) => {
            item.progress = p;
            notify();
          },
        });
        item.mediaId = mediaId;
      } catch (e) {
        item.error = e instanceof Error ? e.message : String(e);
        item.progress = { phase: e instanceof DuplicateError ? "duplicate" : "failed", fraction: 1, message: item.error };
      }
      notify();
    }
  };

  await Promise.all(Array.from({ length: Math.min(concurrency, items.length) }, worker));
  return items;
}
