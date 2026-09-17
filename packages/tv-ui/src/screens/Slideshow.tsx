import { useCallback, useEffect, useMemo, useState } from "react";
import { Text, View } from "react-native";
import { slides as slidesOf, type Trip, type TvApi } from "../api/client";
import { type AudioPlayer } from "../audio";
import { FocusButton } from "../components/FocusButton";
import { KenBurns } from "../components/KenBurns";
import { RemoteImage } from "../components/RemoteImage";
import { Eyebrow, Meta } from "../components/Type";
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
  audio,
  narration = false,
  startAt,
}: {
  api: TvApi;
  token: string;
  trip: Trip;
  onBack: () => void;
  /** Media id to open at: "play this photo on the TV" from the phone. */
  startAt?: string;
  onPresence?: (now: NowShowing) => void;
  /** The latest answer card from the feed, drawn over the photo (F-405). */
  answer?: AnswerCard | null;
  intervalMs?: number;
  /** Platform audio, when the shell has one; captions otherwise (F-709). */
  audio?: AudioPlayer | null;
  /** Read the day's story: audio when possible, captions always. */
  narration?: boolean;
}) {
  const slides = useMemo(() => slidesOf(trip), [trip]);
  const [index, setIndex] = useState(() => Math.max(0, startAt ? slides.findIndex((s) => s.media === startAt) : 0));
  // A second cast while the show is already open jumps to the new photo.
  useEffect(() => {
    if (!startAt) return;
    const at = slides.findIndex((s) => s.media === startAt);
    if (at >= 0) setIndex(at);
  }, [startAt, slides]);
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
  const [captions, setCaptions] = useState(narration);
  const [spoken, setSpoken] = useState<number | null>(null);
  // F-709: on a new day, play its narration if the platform can; captions carry the text.
  useEffect(() => {
    if (!narration || !current || current.day === spoken) return;
    setSpoken(current.day);
    if (!audio || trip.id.startsWith("playlist:")) return;
    api
      .narrationUrl(token, trip.id, current.day)
      .then((r) => audio.play(r.url))
      .catch(() => undefined);
  }, [narration, current, spoken, audio, api, token, trip.id]);
  useEffect(() => () => audio?.stop(), [audio]);

  if (!current) {
    return (
      <View style={{ flex: 1, backgroundColor: theme.bg, padding: theme.safe.x, justifyContent: "center", gap: theme.gap }}>
        <Meta testID="empty" size={theme.body}>
          Nothing to show yet.
        </Meta>
        <FocusButton label="Back" onPress={onBack} preferred testID="back" />
      </View>
    );
  }
  const dayInfo = trip.days.find((d) => d.index === current.day);
  const caption = captions && dayInfo?.story ? dayInfo.story : null;
  const dayCount = new Set(slides.map((s) => s.day)).size;
  const dayLabel = dayInfo?.title ?? `Day ${current.day}`;
  return (
    <View style={{ flex: 1, backgroundColor: "#000" }}>
      <KenBurns slideKey={current.media} paused={paused} durationMs={intervalMs}>
        <RemoteImage api={api} token={token} mediaId={current.media} rendition="tv" width={1920} height={1080} radius={0} />
      </KenBurns>
      <View pointerEvents="none" style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 300, backgroundColor: "#000", opacity: 0.55 }} />
      {answer && (
        <View testID="answer-card" style={{ position: "absolute", top: theme.safe.y, left: theme.safe.x, width: 760, padding: 24, borderRadius: 20, backgroundColor: "#0b0b0fcc", borderWidth: 1, borderColor: "#ffffff1f", gap: 8 }}>
          <Meta size={theme.eyebrow}>Asked from a phone</Meta>
          <Meta size={theme.small}>{`"${answer.question}"`}</Meta>
          <Text testID="answer" style={{ color: theme.fg, fontSize: 28, lineHeight: 38 }}>
            {answer.answer}
          </Text>
        </View>
      )}
      {caption && (
        <View testID="caption-story" style={{ position: "absolute", left: theme.safe.x, right: theme.safe.x, bottom: 230, padding: 22, borderRadius: 20, backgroundColor: "#0b0b0fb3" }}>
          <Text style={{ color: theme.fg, fontSize: theme.heading, lineHeight: 40 }}>{caption}</Text>
        </View>
      )}
      <View style={{ position: "absolute", left: theme.safe.x, right: theme.safe.x, bottom: theme.safe.y, flexDirection: "row", alignItems: "flex-end", justifyContent: "space-between", gap: 24 }}>
        <View style={{ gap: 10, flex: 1 }}>
          <Eyebrow>{`${trip.name} · day ${current.day} of ${dayCount}`}</Eyebrow>
          <Text style={{ color: theme.fg, fontSize: 44, fontWeight: "700" }}>{dayLabel}</Text>
          <Text testID="caption" style={{ color: theme.muted, fontSize: theme.small }}>
            {`${dayLabel} · ${index + 1} of ${slides.length}${paused ? " · paused" : ""}`}
          </Text>
        </View>
        <View style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
          <FocusButton label={paused ? "Play" : "Pause"} onPress={togglePause} preferred testID="playpause" />
          <FocusButton label="Previous day" variant="ghost" onPress={() => jumpDay(-1)} testID="prev-day" />
          <FocusButton label="Next day" variant="ghost" onPress={() => jumpDay(1)} testID="next-day" />
          {dayInfo?.story && <FocusButton label={captions ? "Captions off" : "Captions"} variant="ghost" onPress={() => setCaptions((c) => !c)} testID="captions" />}
          <FocusButton label="Back" variant="ghost" onPress={onBack} testID="back" />
        </View>
      </View>
      <View pointerEvents="none" style={{ position: "absolute", left: theme.safe.x, right: theme.safe.x, bottom: theme.safe.y - 16, height: 4, borderRadius: 2, backgroundColor: "#ffffff33" }}>
        <View style={{ width: `${Math.round(((index + 1) / slides.length) * 100)}%`, height: 4, borderRadius: 2, backgroundColor: theme.accent }} />
      </View>
    </View>
  );
}
