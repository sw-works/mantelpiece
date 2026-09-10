import { KeyValueSessionStore, MemorySessionStore, SESSION_KEY, type AsyncStorageLike } from "../src/session";

class FakeKv implements AsyncStorageLike {
  map = new Map<string, string>();
  async getItem(key: string) {
    return this.map.get(key) ?? null;
  }
  async setItem(key: string, value: string) {
    this.map.set(key, value);
  }
  async removeItem(key: string) {
    this.map.delete(key);
  }
}

describe("session stores", () => {
  const session = { token: "tv-token", household: "h", deviceId: "tv-abc" };

  it("persists the session as JSON under one key and clears it", async () => {
    const kv = new FakeKv();
    const store = new KeyValueSessionStore(kv);
    expect(await store.load()).toBeNull();
    await store.save(session);
    expect(JSON.parse(kv.map.get(SESSION_KEY)!)).toEqual(session);
    expect(await new KeyValueSessionStore(kv).load()).toEqual(session);
    await store.clear();
    expect(await store.load()).toBeNull();
  });

  it("treats corrupt or partial records as signed out", async () => {
    const kv = new FakeKv();
    kv.map.set(SESSION_KEY, "{not json");
    expect(await new KeyValueSessionStore(kv).load()).toBeNull();
    kv.map.set(SESSION_KEY, JSON.stringify({ token: "t" }));
    expect(await new KeyValueSessionStore(kv).load()).toBeNull();
  });

  it("memory store forgets on a new instance", async () => {
    const store = new MemorySessionStore();
    await store.save(session);
    expect(await store.load()).toEqual(session);
    expect(await new MemorySessionStore().load()).toBeNull();
  });
});
