// Pairing a phone with the TV: the phone scans the code, gives a name, and waits until
// someone at the TV lets it in. The waiting request survives a reload.

import type { Api, PairingStatus } from "./api.js";

const WAITING = "ml.pair.waiting";
export const NAME_KEY = "ml.pair.name";

export interface Waiting {
  code: string;
  token: string;
  name: string;
  tvName: string;
}

export function saveWaiting(storage: Storage, waiting: Waiting): void {
  storage.setItem(WAITING, JSON.stringify(waiting));
  storage.setItem(NAME_KEY, waiting.name);
}

/** The request this phone already made for `code`, if any. */
export function readWaiting(storage: Storage, code: string): Waiting | null {
  try {
    const waiting = JSON.parse(storage.getItem(WAITING) ?? "null") as Waiting | null;
    return waiting && waiting.code === code ? waiting : null;
  } catch {
    return null;
  }
}

export function clearWaiting(storage: Storage): void {
  storage.removeItem(WAITING);
}

export type Outcome = Exclude<PairingStatus, { state: "pending" }> | { state: "expired" };

/**
 * Ask until the TV decides. The API holds each call, so `pauseMs` only spaces out quick
 * answers and failures. A rejected waiting token means the request is gone: expired.
 */
export async function untilDecided(
  api: Pick<Api, "pairingStatus">,
  token: string,
  opts: { pauseMs?: number; sleep?: (ms: number) => Promise<void>; signal?: { aborted: boolean } } = {},
): Promise<Outcome | null> {
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  while (!opts.signal?.aborted) {
    try {
      const status = await api.pairingStatus(token);
      if (status.state !== "pending") return status;
    } catch (e) {
      if ((e as { status?: number }).status === 401) return { state: "expired" };
    }
    await sleep(opts.pauseMs ?? 1500);
  }
  return null;
}
