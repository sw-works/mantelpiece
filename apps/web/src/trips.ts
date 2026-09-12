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
