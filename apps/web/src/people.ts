// Pure helpers for the People page (F-704). The consent copy says exactly what is stored
// and what "off" does, because faces are the one thing here a family may not want kept.

import type { PersonView } from "./api.js";

export const CONSENT = {
  off: "Face grouping is off and nothing about faces is stored. Turn it on from People on your TV.",
  on: "Face grouping is on. Face signatures from your trips are kept in a private collection for this household only, are never used to train anything, and are deleted the moment it is turned off on the TV.",
} as const;

/** "Maya · 12 photos" or "Unnamed · 3 photos". */
export function personLabel(p: PersonView): string {
  return `${p.name ?? "Unnamed"} · ${p.media_count} photo${p.media_count === 1 ? "" : "s"}`;
}

/** Clusters to show: named first, then by size. */
export function sortPeople(people: PersonView[]): PersonView[] {
  return [...people].sort((a, b) => Number(Boolean(b.name)) - Number(Boolean(a.name)) || b.media_count - a.media_count || a.id.localeCompare(b.id));
}

/** Everything but `id`, as merge targets. */
export function mergeTargets(people: PersonView[], id: string): PersonView[] {
  return sortPeople(people).filter((p) => p.id !== id);
}
