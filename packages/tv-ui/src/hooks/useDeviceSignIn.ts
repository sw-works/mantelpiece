// F-401: issue a code, show it, poll until the phone approves, hand back the session.

import { useCallback, useEffect, useRef, useState } from "react";
import { type TvApi } from "../api/client";

export type SignInState =
  | { phase: "issuing" }
  | { phase: "waiting"; code: string; link: string; expiresAt: number }
  /** The code's ten minutes ran out; `restart` issues a new one. */
  | { phase: "expired"; code: string }
  | { phase: "approved"; token: string; household: string }
  | { phase: "error"; message: string };

export interface SignInOptions {
  api: TvApi;
  deviceId: string;
  /** Shown on the phone and kept as the household name. */
  tvName?: string;
  /** Delay between polls when the server answers pending immediately. */
  retryMs?: number;
  sleep?: (ms: number) => Promise<void>;
  now?: () => number;
}

export function useDeviceSignIn(opts: SignInOptions): SignInState & { restart: () => void } {
  const [state, setState] = useState<SignInState>({ phase: "issuing" });
  const [attempt, setAttempt] = useState(0);
  const alive = useRef(true);
  // Callers may pass fresh closures on every render; only the api, device and an explicit
  // restart should start a new code.
  const latest = useRef(opts);
  latest.current = opts;

  useEffect(() => {
    alive.current = true;
    const sleep = latest.current.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)));
    const now = () => (latest.current.now ?? Date.now)();
    const retryMs = latest.current.retryMs ?? 2000;
    const tvName = latest.current.tvName;
    (async () => {
      setState({ phase: "issuing" });
      let issued;
      try {
        issued = await opts.api.issueDeviceCode(opts.deviceId, tvName);
      } catch (e) {
        if (alive.current) setState({ phase: "error", message: e instanceof Error ? e.message : String(e) });
        return;
      }
      if (!alive.current) return;
      const expiresAt = Date.parse(issued.expires_at);
      setState({ phase: "waiting", code: issued.code, link: issued.link, expiresAt });
      while (alive.current) {
        if (Number.isFinite(expiresAt) && now() >= expiresAt) {
          setState({ phase: "expired", code: issued.code });
          return;
        }
        let poll;
        try {
          poll = await opts.api.pollDeviceCode(issued.code, opts.deviceId);
        } catch (e) {
          if (alive.current) setState({ phase: "error", message: e instanceof Error ? e.message : String(e) });
          return;
        }
        if (!alive.current) return;
        if (poll.state === "approved") {
          setState({ phase: "approved", token: poll.token, household: poll.household });
          return;
        }
        if (poll.state === "expired") {
          setState({ phase: "expired", code: issued.code });
          return;
        }
        await sleep(retryMs);
      }
    })();
    return () => {
      alive.current = false;
    };
  }, [opts.api, opts.deviceId, attempt]);

  const restart = useCallback(() => setAttempt((a) => a + 1), []);
  return { ...state, restart };
}
