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
  /** The newest `play.requested` seen: the TV opens it once (F-405 by voice). */
  play: PlayRequest | null;
  /** Photos on their way in from a phone; cleared once a trip is detected (F-102). */
  upload?: UploadProgress | null;
  /** Bumped when a phone asks to pair; the approval queue refetches on it. */
  phonesVersion?: number;
  /** The newest batch of photos a phone sent that the TV can show (family channel). */
  arrival?: Arrival | null;
  /** A phone broadcasting to this TV right now (live from the event). */
  live?: LiveNow | null;
  /** Phones asking to go live, oldest first; the TV grants or declines each. */
  liveRequests?: LiveAsk[];
}

export interface LiveAsk {
  id: string;
  from: string;
  /** The TV stops showing the prompt then; the API refuses a late grant. */
  expiresAt: string;
  /** Who is live now: granting ends their broadcast. */
  replacing: string | null;
}

export interface Arrival {
  seq: number;
  /** Event time, so a cold start's replay does not pop old arrivals up. */
  at: string;
  batch: string;
  contributor: string;
  from: string;
  media: string[];
  count: number;
  /** The batch size the phone announced, when its upload progress said so. */
  expected: number | null;
}

export interface LiveNow {
  seq: number;
  at: string;
  id: string;
  from: string;
  url: string;
}

export interface UploadProgress {
  done: number;
  total: number;
  from: string | null;
}

export interface PlayRequest {
  seq: number;
  trip: string | null;
  playlist: string | null;
  name: string;
  source: string;
  /** Media id to open the show at, when the phone cast one photo. */
  start: string | null;
}

const MAX_RECENT = 20;

export function describe(event: FeedEvent): string | null {
  switch (event.type) {
    case "contribution.received":
      return `${String(event.display_name)} added ${String(event.count)} photos`;
    // phone.requested has no ticker line: the approval queue opens instead, and a replayed
    // request would otherwise sit on the home screen long after it was answered.
    case "photos.arrived":
      return `${String(event.display_name)} sent ${String(event.count)} photo${Number(event.count) === 1 ? "" : "s"}`;
    case "live.started":
      return `${String(event.display_name)} is live`;
    case "live.ended":
      return "Live broadcast ended";
    case "upload.progress":
      return Number(event.done) >= Number(event.total) ? `Uploaded ${String(event.total)}, organising…` : `Uploading ${String(event.done)} of ${String(event.total)}`;
    case "trip.detected":
      return `New memory: ${String(event.new_media)} photos organised`;
    case "memory.ready":
      return "Memory ready";
    case "playlist.created":
      return `Playlist ready: ${String(event.name)}`;
    case "play.requested":
      return `Playing ${String(event.name)}`;
    case "dedup.proposal":
      return `${String(event.removable)} duplicates found to review`;
    case "cleanup.proposal":
      return `${String(event.count)} photos to tidy up`;
    case "membership.joined":
      return `${String(event.member_name)} joined`;
    case "share.join_code":
      return `${String(event.recipient)} has a Mantelpiece TV: they can join with code ${String(event.code)}`;
    case "memories.ready":
      return "New memories for today";
    case "answer.card":
      return `Answer: ${String(event.answer)}`;
    case "reaction.received": {
      const who = String(event.actor_name);
      const target = String(event.target);
      if (event.kind === "comment") return `${who}: ${String(event.text ?? "")}`;
      return target.startsWith("day:") ? `${who} loved day ${target.slice(4)}` : `${who} loved a photo`;
    }
    default:
      return null;
  }
}

export function reduce(state: FeedState, events: FeedEvent[], next: number): FeedState {
  if (events.length === 0) return state.since === next ? state : { ...state, since: next };
  const recent = [...state.recent, ...events].slice(-MAX_RECENT);
  const last = events[events.length - 1]!;
  const touchesTrips = events.some((e) => ["trip.detected", "memory.ready", "media.removed", "contribution.received", "membership.joined"].includes(e.type));
  const asked = [...events].reverse().find((e) => e.type === "play.requested");
  const progress = [...events].reverse().find((e) => e.type === "upload.progress");
  const settled = events.some((e) => e.type === "trip.detected" || e.type === "memory.ready");
  const upload: UploadProgress | null = settled
    ? null
    : progress
      ? { done: Number(progress.done), total: Number(progress.total), from: typeof progress.display_name === "string" ? progress.display_name : null }
      : (state.upload ?? null);
  const play: PlayRequest | null = asked
    ? { seq: asked.seq, trip: typeof asked.trip === "string" ? asked.trip : null, playlist: typeof asked.playlist === "string" ? asked.playlist : null, name: String(asked.name ?? ""), source: String(asked.source ?? ""), start: typeof asked.start === "string" ? asked.start : null }
    : state.play;
  return {
    since: next,
    arrival: nextArrival(state.arrival ?? null, events, recent),
    live: nextLive(state.live ?? null, events),
    liveRequests: nextRequests(state.liveRequests ?? [], events),
    recent,
    ticker: describe(last) ?? state.ticker,
    tripsVersion: touchesTrips ? state.tripsVersion + 1 : state.tripsVersion,
    upload,
    play,
    phonesVersion: (state.phonesVersion ?? 0) + (events.some((e) => e.type === "phone.requested") ? 1 : 0),
  };
}

/**
 * Signal runs per photo in parallel, so one batch can announce the same count twice or out
 * of order: keep the larger one. A different batch replaces it. Upload progress from the
 * same phone says how big the batch will be.
 */
function nextArrival(current: Arrival | null, events: FeedEvent[], recent: FeedEvent[]): Arrival | null {
  // The API reports progress as each upload completes, before Signal announces the photo.
  const announced = (contributor: string): number | null => {
    const p = [...recent].reverse().find((e) => e.type === "upload.progress" && String(e.contributor) === contributor);
    return p ? Number(p.total) : null;
  };
  let arrival = current;
  for (const e of events) {
    if (e.type === "photos.arrived") {
      const contributor = String(e.contributor);
      const next: Arrival = {
        seq: e.seq,
        at: e.at,
        batch: String(e.batch),
        contributor,
        from: String(e.display_name),
        media: Array.isArray(e.media) ? e.media.map(String) : [],
        count: Number(e.count),
        expected: announced(contributor),
      };
      if (arrival && arrival.batch === next.batch && arrival.count > next.count) continue;
      arrival = next;
    } else if (e.type === "upload.progress" && arrival && String(e.contributor) === arrival.contributor) {
      arrival = { ...arrival, expected: Number(e.total) };
    }
  }
  return arrival;
}

function nextRequests(current: LiveAsk[], events: FeedEvent[]): LiveAsk[] {
  let asks = current;
  for (const e of events) {
    if (e.type === "live.requested") {
      asks = [...asks.filter((a) => a.id !== String(e.request)), { id: String(e.request), from: String(e.display_name), expiresAt: String(e.expires_at), replacing: typeof e.replacing === "string" ? e.replacing : null }];
    } else if (e.type === "live.request_closed") {
      asks = asks.filter((a) => a.id !== String(e.request));
    }
  }
  return asks;
}

function nextLive(current: LiveNow | null, events: FeedEvent[]): LiveNow | null {
  let live = current;
  for (const e of events) {
    if (e.type === "live.started") live = { seq: e.seq, at: e.at, id: String(e.broadcast), from: String(e.display_name), url: String(e.playback_url) };
    else if (e.type === "live.ended" && live && live.id === String(e.broadcast)) live = null;
  }
  return live;
}

export function useEvents(api: TvApi, token: string | null, opts: { errorBackoffMs?: number; sleep?: (ms: number) => Promise<void> } = {}): FeedState {
  const [state, dispatch] = useReducer(
    (s: FeedState, a: { events: FeedEvent[]; next: number }) => reduce(s, a.events, a.next),
    { since: 0, recent: [], ticker: null, tripsVersion: 0, play: null },
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
