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

  it("raises ApiError with the backend message", async () => {
    const { api } = stub(403, { error: "not your code" });
    await expect(api.pollDeviceCode("X", "tv")).rejects.toMatchObject({ status: 403, message: "not your code" });
  });
});
