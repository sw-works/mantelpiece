// What the upload page shows, decided without a DOM: the counts, which files get a row
// of their own, and the lines of text. A batch can be hundreds of photos, so the page
// never lists every file while sending: only the few in flight and any that failed.

import { type QueueItem } from "./queue.js";

export interface Tally {
  total: number;
  done: number;
  duplicate: number;
  failed: number;
  /** On the TV: sent now, or there already. */
  sent: number;
  /** Overall progress, 0 to 1. */
  fraction: number;
  bytesLeft: number;
  removed: number;
}

/** What the queue adds up to, for the title, the overall bar and the summary line. */
export function tally(items: readonly QueueItem[], removed: ReadonlySet<string>): Tally {
  let done = 0;
  let duplicate = 0;
  let failed = 0;
  let fraction = 0;
  let bytesLeft = 0;
  let removedCount = 0;
  for (const it of items) {
    const phase = it.progress.phase;
    if (phase === "done") done++;
    else if (phase === "duplicate") duplicate++;
    else if (phase === "failed") failed++;
    const f = phase === "uploading" ? it.progress.fraction : phase === "hashing" ? 0 : 1;
    fraction += f;
    bytesLeft += Math.round(it.file.size * (1 - f));
    if (it.mediaId && removed.has(it.mediaId)) removedCount++;
  }
  const total = items.length;
  return { total, done, duplicate, failed, sent: done + duplicate, fraction: total ? fraction / total : 0, bytesLeft, removed: removedCount };
}

/** Files being checked or sent right now: the only rows the page shows while sending. */
export function inFlight(items: readonly QueueItem[]): QueueItem[] {
  return items.filter((it) => it.progress.phase === "uploading" || (it.progress.phase === "hashing" && it.progress.fraction > 0));
}

export function failures(items: readonly QueueItem[]): QueueItem[] {
  return items.filter((it) => it.progress.phase === "failed");
}

/** This phone's photos that reached the TV and were not removed, in the order chosen. */
export function kept(items: readonly QueueItem[], removed: ReadonlySet<string>): QueueItem[] {
  return items.filter((it) => it.progress.phase === "done" && it.mediaId !== undefined && !removed.has(it.mediaId));
}

/** The newest `n` sent, newest first, for the strip under the progress bar. */
export function latestSent(items: readonly QueueItem[], n: number): QueueItem[] {
  return items.filter((it) => it.progress.phase === "done").slice(-n).reverse();
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** "12 of 40 sent · 2 already on the TV": counts that are zero say nothing. */
export function summaryLine(t: Tally): string {
  const parts: string[] = [];
  const added = t.done - t.removed;
  if (added > 0) parts.push(`${plural(added, "photo")} added`);
  if (t.duplicate > 0) parts.push(`${t.duplicate} already on the TV`);
  if (t.removed > 0) parts.push(`${t.removed} removed`);
  if (t.failed > 0) parts.push(`${t.failed} did not send`);
  return parts.join(" · ");
}

/** Title while sending. */
export function sendingTitle(t: Tally): string {
  return `${t.sent} of ${t.total} sent`;
}

/** Title when the batch is over. */
export function doneTitle(t: Tally): string {
  const on = t.sent - t.removed;
  if (t.failed > 0) return `${on} of ${t.total} on the TV`;
  if (t.removed > 0) return `${on} on the TV`;
  return on === 1 ? "Your photo is on the TV" : `All ${on} on the TV`;
}

/** The share-sheet route exists only where the page can be a share target: Android. */
export function shareHint(userAgent: string): string | null {
  return /Android/i.test(userAgent) ? "Add this page to your home screen and Mantelpiece shows up when you share photos." : null;
}

/** How many review tiles to show at first, and per "Show more". */
export const REVIEW_PAGE = 24;
