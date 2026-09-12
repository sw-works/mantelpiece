// Photos shared from the phone's share sheet (Web Share Target). The service worker
// answers the share POST by parking each file in the `memorylane-shared` cache and sending
// the browser to `/?shared=<n>`; the uploader page takes them out here and uploads them
// like a picker selection. Android Chrome only: iOS Safari has no share target, so iPhones
// keep the QR and the picker.

export const SHARED_CACHE = "memorylane-shared";

/** The subset of the Cache API this needs, so tests can pass a plain object. */
export interface SharedCache {
  keys(): Promise<ReadonlyArray<{ url: string }>>;
  match(url: string): Promise<Response | undefined>;
  delete(url: string): Promise<boolean>;
}

/** How many files the share sheet handed over, from `/?shared=<n>`; 0 for any other visit. */
export function sharedCount(url: URL): number {
  const n = Number(url.searchParams.get("shared") ?? "0");
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : 0;
}

/**
 * Take every parked file out of the cache, oldest first, as `File`s the upload queue can
 * hash and send. The cache is emptied so a reload does not upload twice.
 */
export async function takeSharedFiles(cache: SharedCache | undefined): Promise<File[]> {
  if (!cache) return [];
  const keys = (await cache.keys()).slice().sort((a, b) => a.url.localeCompare(b.url));
  const files: File[] = [];
  for (const key of keys) {
    const res = await cache.match(key.url);
    await cache.delete(key.url);
    if (!res) continue;
    const name = decodeURIComponent(res.headers.get("x-file-name") ?? "") || `shared-${files.length + 1}`;
    const type = res.headers.get("content-type") ?? "application/octet-stream";
    const lastModified = Number(res.headers.get("x-file-modified") ?? Date.now());
    files.push(new File([await res.blob()], name, { type, lastModified }));
  }
  return files;
}

/** Open the shared cache when the browser has one; `undefined` elsewhere (tests, old browsers). */
export async function openSharedCache(): Promise<SharedCache | undefined> {
  if (typeof caches === "undefined") return undefined;
  return caches.open(SHARED_CACHE);
}
