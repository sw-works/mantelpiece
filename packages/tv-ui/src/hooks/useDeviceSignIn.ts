// F-401: issue a code, show it, poll until the phone approves, hand back the session.

import { useCallback, useEffect, useRef, useState } from "react";
import { type TvApi } from "../api/client";

export type SignInState =
  | { phase: "issuing" }
  | { phase: "waiting"; code: string; link: string }
  | { phase: "approved"; token: string; household: string }
  | { phase: "error"; message: string };

export interface SignInOptions {
  api: TvApi;
  deviceId: string;
  /** Delay between polls when the server answers pending immediately. */
  retryMs?: number;
  sleep?: (ms: number) => Promise<void>;
}

export function useDeviceSignIn(opts: SignInOptions): SignInState & { restart: () => void } {
  const [state, setState] = useState<SignInState>({ phase: "issuing" });
  const [attempt, setAttempt] = useState(0);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    const sleep = opts.sleep ?? ((ms) => new Promise((r) => setTimeout(r, ms)));
    const retryMs = opts.retryMs ?? 2000;
    (async () => {
      setState({ phase: "issuing" });
      let issued;
      try {
        issued = await opts.api.issueDeviceCode(opts.deviceId);
      } catch (e) {
        if (alive.current) setState({ phase: "error", message: e instanceof Error ? e.message : String(e) });
        return;
      }
      if (!alive.current) return;
      setState({ phase: "waiting", code: issued.code, link: issued.link });
      while (alive.current) {
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
          setState({ phase: "error", message: "Code expired. Press select to get a new one." });
          return;
        }
        await sleep(retryMs);
      }
    })();
    return () => {
      alive.current = false;
    };
  }, [opts.api, opts.deviceId, opts.retryMs, opts.sleep, attempt]);

  const restart = useCallback(() => setAttempt((a) => a + 1), []);
  return { ...state, restart };
}
