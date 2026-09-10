import { useCallback, useEffect, useMemo, useState } from "react";
import { Text, View } from "react-native";
import { slides as slidesOf, type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { KenBurns } from "../components/KenBurns";
import { RemoteImage } from "../components/RemoteImage";
import { useRemoteKeys } from "../hooks/useRemoteKeys";
import { theme } from "../theme";

export interface NowShowing {
  media: string;
  day: number;
  paused: boolean;
}

export interface AnswerCard {
  question: string;
  answer: string;
  media?: string | null;
}

/**
 * F-403: full-screen slideshow through a trip in day order. Play, pause, next and
 * previous day on the remote; what is on screen and whether it is paused is reported
 * through `onPresence` so a voice question can refer to "that photo" (research R3).
 */
export function Slideshow({
  api,
  token,
  trip,
  onBack,
  onPresence,
  answer,
  intervalMs = 6000,
}: {
  api: TvApi;
  token: string;
  trip: Trip;
  onBack: () => void;
  onPresence?: (now: NowShowing) => void;
  /** The latest answer card from the feed, drawn over the photo (F-405). */
  answer?: AnswerCard | null;
  intervalMs?: number;
}) {
  const slides = useMemo(() => slidesOf(trip), [trip]);
  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);
  const current = slides[index];

  useEffect(() => {
    if (paused || slides.length < 2) return;
    const t = setInterval(() => setIndex((i) => (i + 1) % slides.length), intervalMs);
    return () => clearInterval(t);
  }, [paused, slides.length, intervalMs]);

  useEffect(() => {
    if (current) onPresence?.({ media: current.media, day: current.day, paused });
  }, [current, paused, onPresence]);

  const jumpDay = useCallback(
    (direction: 1 | -1) => {
      if (!current) return;
      const days = [...new Set(slides.map((s) => s.day))];
      const at = days.indexOf(current.day);
      const next = days[(at + direction + days.length) % days.length];
      setIndex(slides.findIndex((s) => s.day === next));
    },
    [current, slides],
  );
  const togglePause = useCallback(() => setPaused((p) => !p), []);
  useRemoteKeys({ onBack, onPlayPause: togglePause });

  if (!current) {
    return (
      <View style={{ flex: 1, backgroundColor: theme.bg, padding: theme.safe.x, justifyContent: "center" }}>
        <Text testID="empty" style={{ color: theme.muted, fontSize: theme.body }}>Nothing to show yet.</Text>
        <FocusButton label="Back" onPress={onBack} preferred testID="back" />
      </View>
    );
  }
  const dayInfo = trip.days.find((d) => d.index === current.day);
  return (
    <View style={{ flex: 1, backgroundColor: "#000" }}>
      <KenBurns slideKey={current.media} paused={paused} durationMs={intervalMs}>
        <RemoteImage api={api} token={token} mediaId={current.media} rendition="tv" width={1920} height={1080} />
      </KenBurns>
      {answer && (
        <View testID="answer-card" style={{ position: "absolute", top: theme.safe.y, left: theme.safe.x, right: theme.safe.x, padding: 24, borderRadius: theme.radius, backgroundColor: "#000c", gap: 8 }}>
          <Text style={{ color: theme.muted, fontSize: theme.small }}>{answer.question}</Text>
          <Text testID="answer" style={{ color: theme.fg, fontSize: theme.body }}>{answer.answer}</Text>
        </View>
      )}
      <View style={{ position: "absolute", left: theme.actionSafe.x, right: theme.actionSafe.x, bottom: theme.actionSafe.y, gap: 12 }}>
        <Text testID="caption" style={{ color: theme.fg, fontSize: theme.small }}>
          {`${dayInfo?.title ?? `Day ${current.day}`} · ${index + 1} of ${slides.length}${paused ? " · paused" : ""}`}
        </Text>
        <View style={{ flexDirection: "row", gap: 12 }}>
          <FocusButton label={paused ? "Play" : "Pause"} onPress={togglePause} preferred testID="playpause" />
          <FocusButton label="Previous day" onPress={() => jumpDay(-1)} testID="prev-day" />
          <FocusButton label="Next day" onPress={() => jumpDay(1)} testID="next-day" />
          <FocusButton label="Back" onPress={onBack} testID="back" />
        </View>
      </View>
    </View>
  );
}
