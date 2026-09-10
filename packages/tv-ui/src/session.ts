// Where the TV keeps its session. The platform shell supplies a key-value store (on Vega
// the system-deployed AsyncStorage, research R2); tests and unknown platforms fall back to
// memory and re-pair after a cold start.

export interface SessionStore {
  load(): Promise<{ token: string; household: string; deviceId: string } | null>;
  save(session: { token: string; household: string; deviceId: string }): Promise<void>;
  clear(): Promise<void>;
}

export class MemorySessionStore implements SessionStore {
  private value: { token: string; household: string; deviceId: string } | null = null;
  async load() {
    return this.value;
  }
  async save(session: { token: string; household: string; deviceId: string }) {
    this.value = session;
  }
  async clear() {
    this.value = null;
  }
}

/** The subset of AsyncStorage the store needs; any platform key-value store fits. */
export interface AsyncStorageLike {
  getItem(key: string): Promise<string | null>;
  setItem(key: string, value: string): Promise<void>;
  removeItem(key: string): Promise<void>;
}

export const SESSION_KEY = "memorylane.session.v1";

/** Persists the session as JSON under one key. Corrupt or partial records read as signed out. */
export class KeyValueSessionStore implements SessionStore {
  constructor(
    private readonly kv: AsyncStorageLike,
    private readonly key: string = SESSION_KEY,
  ) {}
  async load() {
    const raw = await this.kv.getItem(this.key);
    if (!raw) return null;
    try {
      const v = JSON.parse(raw) as Partial<Record<"token" | "household" | "deviceId", unknown>>;
      if (typeof v.token === "string" && typeof v.household === "string" && typeof v.deviceId === "string") {
        return { token: v.token, household: v.household, deviceId: v.deviceId };
      }
    } catch {
      // fall through: unreadable record means signed out
    }
    return null;
  }
  async save(session: { token: string; household: string; deviceId: string }) {
    await this.kv.setItem(this.key, JSON.stringify(session));
  }
  async clear() {
    await this.kv.removeItem(this.key);
  }
}

export function newDeviceId(random: () => number = Math.random): string {
  const alphabet = "abcdefghijklmnopqrstuvwxyz0123456789";
  let out = "tv-";
  for (let i = 0; i < 16; i++) out += alphabet[Math.floor(random() * alphabet.length)];
  return out;
}
