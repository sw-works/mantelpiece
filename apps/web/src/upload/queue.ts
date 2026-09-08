// A batch of files with bounded concurrency. Files that fail stay in the list with their
// error; the batch counter still reaches total so the backend starts organising.

import { type Api } from "../api.js";
import { DuplicateError, uploadFile, type UploadProgress } from "./multipart.js";
import { type ResumeStore } from "./store.js";

export interface QueueItem {
  file: File;
  progress: UploadProgress;
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

export async function runQueue(files: File[], opts: QueueOptions): Promise<QueueItem[]> {
  const items: QueueItem[] = files.map((file) => ({ file, progress: { phase: "hashing", fraction: 0 } }));
  const batchId = newBatchId();
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
