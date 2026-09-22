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
export function Live({
  live,
  from,
  player: Player,
  onBack,
  onEnd,
  endedCloseMs = 6000,
}: {
  /** The broadcast, or null once it ended. */
  live: LiveNow | null;
  /** Who was live, kept for the "ended" line after `live` goes null. */
  from: string;
  player?: LivePlayer;
  onBack: () => void;
  onEnd: () => void;
  endedCloseMs?: number;
}) {
  const [state, setState] = useState<LivePlayerState>("starting");
  // Ended from this TV, so the phone did not end it.
  const [endedHere, setEndedHere] = useState(false);
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
  const line = { starting: "Connecting…", playing: "A few seconds behind the room", stalled: "Waiting for the picture…", error: "The picture stopped. It may come back." }[state];
  return (
    <View testID="live" style={{ flex: 1, backgroundColor: "#000" }}>
      {Player ? (
        <Player uri={live.url} onState={setState} />
      ) : (
        <View style={{ flex: 1, justifyContent: "center", alignItems: "center" }}>
          <Meta testID="no-player" size={theme.body}>This TV cannot play live video yet.</Meta>
        </View>
      )}
      <View style={{ position: "absolute", left: theme.safe.x, top: theme.safe.y, flexDirection: "row", alignItems: "center", gap: 14, paddingHorizontal: 20, height: 56, borderRadius: 28, backgroundColor: "#0b0b0fd9" }}>
        <View style={{ width: 14, height: 14, borderRadius: 7, backgroundColor: "#ff4d4d" }} />
        <Meta testID="live-who" color={theme.fg} size={theme.body}>{`Live · ${live.from}`}</Meta>
        <Meta testID="live-state">{line}</Meta>
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
