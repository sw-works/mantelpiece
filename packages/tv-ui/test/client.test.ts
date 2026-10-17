import { TvApi } from "../src/api/client";

function stub(status: number, body: unknown) {
  const calls: { url: string; init?: RequestInit }[] = [];
  const fetchFn = (async (url: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(url), init });
    return new Response(status === 204 ? null : JSON.stringify(body), { status });
  }) as typeof fetch;
  return { api: new TvApi("https://api.test", fetchFn), calls };
}

describe("TvApi", () => {
  it("sends bearer tokens and JSON bodies", async () => {
    const { api, calls } = stub(200, { events: [], next: 3 });
    await api.events("tok", 3);
    expect(calls[0]!.url).toBe("https://api.test/events?since=3");
    expect((calls[0]!.init!.headers as Record<string, string>).authorization).toBe("Bearer tok");
    const posted = stub(201, { code: "X" });
    await posted.api.issueDeviceCode("tv-1");
    expect(posted.calls[0]!.init!.body).toBe(JSON.stringify({ device_id: "tv-1" }));
  });

  it("asks for a reel, a day's film (null when there is none) and a new film", async () => {
    const reel = stub(200, { slides: [] });
    await reel.api.reel("tok", "t1", 2);
    expect(reel.calls[0]!.url).toBe("https://api.test/trips/t1/reel?day=2");
    await reel.api.reel("tok", "t1");
    expect(reel.calls[1]!.url).toBe("https://api.test/trips/t1/reel");
    const none = stub(404, { error: "no film yet" });
    await expect(none.api.film("tok", "t1", 2)).resolves.toBeNull();
    expect(none.calls[0]!.url).toBe("https://api.test/trips/t1/film?day=2");
    const denied = stub(403, { error: "forbidden" });
    await expect(denied.api.film("tok", "t1", 2)).rejects.toMatchObject({ status: 403 });
    const queued = stub(202, { queued: true });
    await expect(queued.api.makeFilm("tok", "t1", 2)).resolves.toEqual({ queued: true });
    expect(queued.calls[0]!.init!.method).toBe("POST");
    expect(queued.calls[0]!.url).toBe("https://api.test/trips/t1/film");
    expect(queued.calls[0]!.init!.body).toBe(JSON.stringify({ day: 2 }));
  });

  it("raises ApiError with the backend message", async () => {
    const { api } = stub(403, { error: "not your code" });
    await expect(api.pollDeviceCode("X", "tv")).rejects.toMatchObject({ status: 403, message: "not your code" });
  });
});
