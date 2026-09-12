// Trips grouped by the year they started, newest year first, for the Home list and the
// year jump. Pure so it can be shared with the phone pages by copy and tested alone.

export interface YearGroup<T> {
  year: string;
  trips: T[];
}

/** Group trips by start year, newest year first; order inside a year is kept as given. */
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
