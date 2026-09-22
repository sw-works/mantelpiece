// Resume records (F-161). A browser cannot re-open a File after reload, so the record is
// keyed by a fingerprint of the file the user picks again: same name, size and
// modification time means the same file, and the completed parts are reused.

export interface ResumeRecord {
  fingerprint: string;
  mediaId: string;
  uploadId: string;
  key: string;
  partSize: number;
  partUrls: string[];
  completed: { part_number: number; etag: string }[];
  sha256: string;
  batchId: string;
  createdAt: number;
}

export interface ResumeStore {
  get(fingerprint: string): Promise<ResumeRecord | undefined>;
  put(record: ResumeRecord): Promise<void>;
  delete(fingerprint: string): Promise<void>;
}

export function fingerprintOf(file: { name: string; size: number; lastModified: number }): string {
  return `${file.name}|${file.size}|${file.lastModified}`;
}

export class MemoryResumeStore implements ResumeStore {
  readonly records = new Map<string, ResumeRecord>();
  async get(fingerprint: string) {
    return this.records.get(fingerprint);
  }
  async put(record: ResumeRecord) {
    this.records.set(record.fingerprint, record);
  }
  async delete(fingerprint: string) {
    this.records.delete(fingerprint);
  }
}

const DB = "mantelpiece";
const STORE = "uploads";

export class IndexedDbResumeStore implements ResumeStore {
  private db: Promise<IDBDatabase>;

  constructor(factory: IDBFactory = indexedDB) {
    this.db = new Promise((resolve, reject) => {
      const req = factory.open(DB, 1);
      req.onupgradeneeded = () => req.result.createObjectStore(STORE, { keyPath: "fingerprint" });
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  }

  private async tx(mode: IDBTransactionMode): Promise<IDBObjectStore> {
    return (await this.db).transaction(STORE, mode).objectStore(STORE);
  }

  private wrap<T>(req: IDBRequest<T>): Promise<T> {
    return new Promise((resolve, reject) => {
      req.onsuccess = () => resolve(req.result);
      req.onerror = () => reject(req.error);
    });
  }

  async get(fingerprint: string) {
    return this.wrap((await this.tx("readonly")).get(fingerprint)) as Promise<ResumeRecord | undefined>;
  }
  async put(record: ResumeRecord) {
    await this.wrap((await this.tx("readwrite")).put(record));
  }
  async delete(fingerprint: string) {
    await this.wrap((await this.tx("readwrite")).delete(fingerprint));
  }
}
