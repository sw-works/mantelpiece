import { describe, expect, it } from "vitest";
import { Api } from "../src/api.js";
import { describeOutcome, nowShowingLine, speechRecognition, transcriptOf } from "../src/ask.js";

describe("phone Ask page", () => {
  it("finds the browser's speech recognition under either name", () => {
    class Fake {}
    expect(speechRecognition({})).toBeNull();
    expect(speechRecognition({ webkitSpeechRecognition: Fake })).toBe(Fake);
    expect(speechRecognition({ SpeechRecognition: Fake, webkitSpeechRecognition: class {} })).toBe(Fake);
    expect(transcriptOf({ results: [[{ transcript: "  what's that statue " }]] })).toBe("what's that statue");
    expect(transcriptOf({ results: [] })).toBe("");
  });

  it("explains each outcome and what the TV shows", () => {
    expect(describeOutcome({ mode: "describe", tv: "tv-1", question: "q", card: { answer: "Todai-ji.", landmark: "Todai-ji", taken_by: "Maya", when: "2026-04-04" } })).toBe("Todai-ji.");
    expect(describeOutcome({ mode: "search", question: "q", playlist: "p", count: 1 })).toBe("Playing 1 photo on the TV.");
    expect(describeOutcome({ mode: "nothing", question: "q", hint: "Try a place." })).toBe("Nothing matched. Try a place.");
    expect(nowShowingLine(undefined)).toMatch(/^Nothing is on the TV/);
    expect(nowShowingLine({ trip: "kyoto", day: 2, paused: true })).toBe("On the TV now: a trip, day 2, paused. Ask about that photo.");
    expect(nowShowingLine({ trip: null, day: null, paused: false })).toBe("On the TV now: a playlist. Ask about that photo.");
  });

  it("posts the question and the TV to /ask with the owner session", async () => {
    const seen: Array<{ url: string; body: unknown; headers: Record<string, string> }> = [];
    const fetchFn = (async (input: RequestInfo | URL, init?: RequestInit) => {
      seen.push({ url: String(input), body: JSON.parse(String(init?.body ?? "null")), headers: (init?.headers as Record<string, string>) ?? {} });
      return new Response(JSON.stringify({ mode: "nothing", question: "x", hint: "h" }), { status: 200 });
    }) as typeof fetch;
    const api = new Api("https://api.test", fetchFn);
    const result = await api.ask("owner-token", "what's that", "tv-1");
    expect(result.mode).toBe("nothing");
    expect(seen[0]!.url).toBe("https://api.test/ask");
    expect(seen[0]!.body).toEqual({ question: "what's that", tv: "tv-1" });
    expect(seen[0]!.headers["authorization"]).toBe("Bearer owner-token");
  });
});
