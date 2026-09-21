// Pairing, TV side: keep a live code on screen and the approval queue current.

import { useCallback, useEffect, useRef, useState } from "react";
import { type PairQueue, type TvApi } from "../api/client";

export interface PairingState {
  /** The code on screen; null while one is being fetched. */
  code: { code: string; link: string } | null;
  queue: PairQueue;
  error: string | null;
  /** Let a phone in (`true`), or turn it away / forget it (`false`). */
  decide: (id: string, allow: boolean) => Promise<void>;
}

const EMPTY: PairQueue = { pending: [], phones: [] };

export function usePairing(opts: { api: TvApi; token: string; deviceId: string; version: number; withCode?: boolean }): PairingState {
  const { api, token, deviceId, version, withCode = true } = opts;
  const [code, setCode] = useState<PairingState["code"]>(null);
  const [queue, setQueue] = useState<PairQueue>(EMPTY);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  // A code lasts ten minutes; fetch the next one a little before it runs out.
  useEffect(() => {
    if (!withCode) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let stopped = false;
    const issue = async () => {
      try {
        const next = await api.pairCode(token, deviceId);
        if (stopped) return;
        setCode({ code: next.code, link: next.link });
        setError(null);
        const left = Date.parse(next.expires_at) - Date.now();
        timer = setTimeout(issue, Number.isFinite(left) ? Math.max(30_000, left - 30_000) : 9 * 60_000);
      } catch (e) {
        if (stopped) return;
        setError(e instanceof Error ? e.message : String(e));
        timer = setTimeout(issue, 5000);
      }
    };
    void issue();
    return () => {
      stopped = true;
      if (timer) clearTimeout(timer);
    };
  }, [api, token, deviceId, withCode]);

  const refresh = useCallback(async () => {
    try {
      const next = await api.pairQueue(token);
      if (alive.current) setQueue(next);
    } catch {
      // the feed brings the next change; keep what is on screen
    }
  }, [api, token]);

  useEffect(() => {
    void refresh();
  }, [refresh, version]);

  const decide = useCallback(
    async (id: string, allow: boolean) => {
      try {
        await api.decidePhone(token, id, allow);
      } catch (e) {
        if (alive.current) setError(e instanceof Error ? e.message : String(e));
      }
      await refresh();
    },
    [api, token, refresh],
  );

  return { code, queue, error, decide };
}
