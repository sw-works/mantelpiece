import { useEffect, useState } from "react";
import { View } from "react-native";
import { FocusButton } from "../components/FocusButton";
import { Eyebrow, Meta, Title } from "../components/Type";
import { type LiveNow } from "../hooks/useEvents";
import { useRemoteKeys } from "../hooks/useRemoteKeys";
import { type LivePlayer, type LivePlayerState } from "../live";
import { theme } from "../theme";

/**
 * Live from the event: a phone's camera on the TV, a few seconds behind. The picture is
 * the platform's player; this screen adds who is live, how far behind it runs, and a way
 * to end it. When the phone stops, it says so and goes back.
 */
/** A picture that stopped: build the player again, this often, this many times. */
export const RETRY_MS = 3000;
export const MAX_RETRIES = 3;

export function Live({
  live,
  from,
  player: Player,
  onBack,
  onEnd,
  endedCloseMs = 6000,
  retryMs = RETRY_MS,
}: {
  /** The broadcast, or null once it ended. */
  live: LiveNow | null;
  /** Who was live, kept for the "ended" line after `live` goes null. */
  from: string;
  player?: LivePlayer;
  onBack: () => void;
  onEnd: () => void;
  endedCloseMs?: number;
  retryMs?: number;
}) {
  const [state, setState] = useState<LivePlayerState>("starting");
  // Ended from this TV, so the phone did not end it.
  const [endedHere, setEndedHere] = useState(false);
  // The platform player can wedge: its pipeline stops taking video while the stream is
  // still running. A fresh player picks the broadcast up again; the old one never recovers.
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (state !== "error" || attempt >= MAX_RETRIES) return;
    const t = setTimeout(() => {
      setState("starting");
      setAttempt((n) => n + 1);
    }, retryMs);
    return () => clearTimeout(t);
  }, [state, attempt, retryMs]);
  // A new broadcast starts its own count.
  useEffect(() => setAttempt(0), [live?.id]);
  useRemoteKeys({ onBack });
  useEffect(() => {
    if (live || endedCloseMs <= 0) return;
    const t = setTimeout(onBack, endedCloseMs);
    return () => clearTimeout(t);
  }, [live, onBack, endedCloseMs]);

  if (!live) {
    return (
      <View testID="live-ended" style={{ flex: 1, backgroundColor: theme.bg, padding: theme.safe.x, justifyContent: "center", gap: theme.gap }}>
        <Eyebrow>Live</Eyebrow>
        <Title>{endedHere ? "Broadcast ended" : `${from} ended the broadcast`}</Title>
        <FocusButton label="Back" onPress={onBack} preferred testID="back" />
      </View>
    );
  }
  const stopped = attempt >= MAX_RETRIES ? "The picture stopped. Press Back and open Live again." : "The picture stopped. Trying again…";
  const line = { starting: attempt > 0 ? "Reconnecting…" : "Connecting…", playing: "A few seconds behind the room", stalled: "Waiting for the picture…", error: stopped }[state];
  return (
    <View testID="live" style={{ flex: 1, backgroundColor: "#000" }}>
      {Player ? (
        <Player key={`${live.id}:${attempt}`} uri={live.url} onState={setState} />
      ) : (
        <View style={{ flex: 1, justifyContent: "center", alignItems: "center" }}>
          <Meta testID="no-player" size={theme.body}>This TV cannot play live video yet.</Meta>
        </View>
      )}
      <View style={{ position: "absolute", left: theme.safe.x, top: theme.safe.y, flexDirection: "row", alignItems: "center", gap: 14, paddingHorizontal: 20, height: 56, borderRadius: 28, backgroundColor: "#0b0b0fd9" }}>
        <View style={{ width: 14, height: 14, borderRadius: 7, backgroundColor: "#ff4d4d" }} />
        <Meta testID="live-who" color={theme.fg} size={theme.body}>{`Live · ${live.from}`}</Meta>
        <Meta testID="live-state">{line}</Meta>
        {live.saving ? <Meta testID="live-saving" color={theme.fg}>· Recording to memories</Meta> : null}
      </View>
      <View style={{ position: "absolute", right: theme.safe.x, bottom: theme.safe.y, flexDirection: "row", gap: 18 }}>
        <FocusButton label="Back" variant="ghost" onPress={onBack} preferred testID="back" />
        <FocusButton label="End broadcast" variant="ghost" onPress={() => {
            setEndedHere(true);
            onEnd();
          }}
          testID="end" />
      </View>
    </View>
  );
}
