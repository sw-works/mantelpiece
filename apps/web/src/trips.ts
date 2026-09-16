// Trips grouped by the year they started, newest year first, for the My trips page.
// Same shape as packages/tv-ui/src/trips.ts; copied because the phone bundle carries no
// React Native code.

export interface YearGroup<T> {
  year: string;
  trips: T[];
}

export function groupByYear<T extends { start: string }>(trips: readonly T[]): YearGroup<T>[] {
  const groups = new Map<string, T[]>();
  for (const t of trips) {
    const year = t.start.slice(0, 4) || "Undated";
    const list = groups.get(year);
    if (list) list.push(t);
    else groups.set(year, [t]);
  }
  return [...groups.entries()].sort((a, b) => b[0].localeCompare(a[0])).map(([year, list]) => ({ year, trips: list }));
}

/** One line under a trip name: dates and how many highlights. */
export function tripSummaryLine(trip: { start: string; end: string; highlights: string[] }): string {
  const days = Math.max(1, Math.round((Date.parse(trip.end) - Date.parse(trip.start)) / 86_400_000) + 1);
  const span = trip.start.slice(0, 10) === trip.end.slice(0, 10) ? trip.start.slice(0, 10) : `${trip.start.slice(0, 10)} to ${trip.end.slice(0, 10)}`;
  return `${span} · ${days} day${days === 1 ? "" : "s"}${trip.highlights.length ? ` · ${trip.highlights.length} highlight${trip.highlights.length === 1 ? "" : "s"}` : ""}`;
}

export interface NowShowing {
  device_id: string;
  media: string;
  trip: string | null;
  day: number | null;
  paused: boolean;
  at: string;
}

/**
 * Chromecast-style confirmation after "Play on TV": the TVs report what they show
 * (`/presence`), so the phone can say which one picked the request up. A record counts
 * when it is for the trip, at the cast photo when one was asked for, and changed since
 * the snapshot taken before the request (no clock comparison, so phone and server clocks
 * need not agree). Returns null while no TV has caught up.
 */
export function castConfirmation(presence: readonly NowShowing[], before: readonly NowShowing[], want: { trip: string; start?: string }): string | null {
  const prior = new Map(before.map((p) => [p.device_id, p]));
  const hits = presence.filter((p) => {
    if (p.trip !== want.trip || (want.start && p.media !== want.start)) return false;
    const was = prior.get(p.device_id);
    return !was || was.at !== p.at || was.media !== p.media;
  });
  if (hits.length === 0) return null;
  const tvs = hits.length === 1 ? "your TV" : `${hits.length} TVs`;
  const first = hits[0]!;
  return `Now showing on ${tvs}${first.day ? `, day ${first.day}` : ""}.`;
}

/** Retry `check` every `everyMs` until it returns a string or `tries` run out. */
export async function untilConfirmed(check: () => Promise<string | null>, tries = 6, everyMs = 1500, sleep: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms))): Promise<string | null> {
  for (let i = 0; i < tries; i++) {
    await sleep(everyMs);
    const line = await check().catch(() => null);
    if (line) return line;
  }
  return null;
}
