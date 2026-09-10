// Voice on the phone (decision log section 7): the browser's speech recognition feeds the
// same /ask the TV and Alexa+ use. No microphone ever opens on the TV.

import type { AskResult } from "./api.js";

/** Minimal shape of the Web Speech API we use. */
export interface RecognitionLike {
  lang: string;
  interimResults: boolean;
  maxAlternatives: number;
  onresult: ((event: { results: ArrayLike<ArrayLike<{ transcript: string }>> }) => void) | null;
  onerror: ((event: { error: string }) => void) | null;
  onend: (() => void) | null;
  start(): void;
  stop(): void;
}

type RecognitionCtor = new () => RecognitionLike;

/** The constructor when the browser has one (Safari and Chrome do), else null. */
export function speechRecognition(win: Record<string, unknown>): RecognitionCtor | null {
  const ctor = (win["SpeechRecognition"] ?? win["webkitSpeechRecognition"]) as RecognitionCtor | undefined;
  return typeof ctor === "function" ? ctor : null;
}

/** First transcript of a recognition result event. */
export function transcriptOf(event: { results: ArrayLike<ArrayLike<{ transcript: string }>> }): string {
  const first = event.results[0]?.[0];
  return first ? first.transcript.trim() : "";
}

/** What to tell the person after /ask answered. */
export function describeOutcome(result: AskResult): string {
  switch (result.mode) {
    case "describe":
      return result.card.answer;
    case "search":
      return `Playing ${result.count} photo${result.count === 1 ? "" : "s"} on the TV.`;
    case "nothing":
      return `Nothing matched. ${result.hint}`;
  }
}

/** One line for the "on the TV now" preview. */
export function nowShowingLine(p: { trip: string | null; day: number | null; paused: boolean } | undefined): string {
  if (!p) return "Nothing is on the TV right now. Ask anyway and the answer becomes a playlist.";
  const where = p.trip ? `a trip${p.day ? `, day ${p.day}` : ""}` : "a playlist";
  return `On the TV now: ${where}${p.paused ? ", paused" : ""}. Ask about that photo.`;
}
