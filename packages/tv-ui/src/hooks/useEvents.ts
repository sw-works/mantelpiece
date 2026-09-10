// F-407: HTTP long-poll of the household feed. The server holds the request up to
// 25 seconds; this loop re-issues it forever with the last seen sequence.

import { useEffect, useReducer, useRef } from "react";
import { type FeedEvent, type TvApi } from "../api/client";

export interface FeedState {
  since: number;
  recent: FeedEvent[];
  /** Human line for the live ticker, e.g. "Maya added 42 photos". */
  ticker: string | null;
  /** Bumped whenever trips may have changed; screens refetch on it. */
  tripsVersion: number;
}

const MAX_RECENT = 20;

export function describe(event: FeedEvent): string | null {
  switch (event.type) {
    case "contribution.received":
      return `${String(event.display_name)} added ${String(event.count)} photos`;
    case "upload.progress":
      return Number(event.done) >= Number(event.total) ? `Uploaded ${String(event.total)}, organising…` : `Uploading ${String(event.done)} of ${String(event.total)}`;
    case "trip.detected":
      return `New memory: ${String(event.new_media)} photos organised`;
    case "memory.ready":
      return "Memory ready";
    case "playlist.created":
      return `Playlist ready: ${String(event.name)}`;
    case "dedup.proposal":
      return `${String(event.removable)} duplicates found to review`;
    case "cleanup.proposal":
      return `${String(event.count)} photos to tidy up`;
    case "membership.joined":
      return `${String(event.member_name)} joined`;
    case "memories.ready":
      return "New memories for today";
    case "answer.card":
      return `Answer: ${String(event.answer)}`;
    default:
      return null;
  }
}

export function reduce(state: FeedState, events: FeedEvent[], next: number): FeedState {
  if (events.length === 0) return state.since === next ? state : { ...state, since: next };
  const recent = [...state.recent, ...events].slice(-MAX_RECENT);
  const last = events[events.length - 1]!;
  const touchesTrips = events.some((e) => ["trip.detected", "memory.ready", "media.removed", "contribution.received", "membership.joined"].includes(e.type));
  return {
    since: next,
    recent,
    ticker: describe(last) ?? state.ticker,
    tripsVersion: touchesTrips ? state.tripsVersion + 1 : state.tripsVersion,
  };
}

export function useEvents(api: TvApi, token: string | null, opts: { errorBackoffMs?: number; sleep?: (ms: number) => Promise<void> } = {}): FeedState {
  const [state, dispatch] = useReducer(
    (s: FeedState, a: { events: FeedEvent[]; next: number }) => reduce(s, a.events, a.next),
    { since: 0, recent: [], ticker: null, tripsVersion: 0 },
  );
  // The loop owns its own cursor: React batches the dispatch, so reading state after it
  // would re-poll the sequence just consumed.
  const sinceRef = useRef(0);

  useEffect(() => {
    if (!token) return;
    let alive = true;
    const sleep = opts.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)));
    (async () => {
      while (alive) {
        try {
          const page = await api.events(token, sinceRef.current);
          if (!alive) return;
          sinceRef.current = page.next;
          dispatch({ events: page.events, next: page.next });
        } catch {
          await sleep(opts.errorBackoffMs ?? 5000);
        }
      }
    })();
    return () => {
      alive = false;
    };
  }, [api, token, opts.errorBackoffMs, opts.sleep]);

  return state;
}
