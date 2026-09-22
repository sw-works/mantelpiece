import { useCallback, useEffect, useRef, useState } from "react";
import { View } from "react-native";
import { type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { KenBurns } from "../components/KenBurns";
import { RemoteImage } from "../components/RemoteImage";
import { Eyebrow, Meta, Title } from "../components/Type";
import { type Arrival } from "../hooks/useEvents";
import { useRemoteKeys } from "../hooks/useRemoteKeys";
import { theme } from "../theme";

/** How many photos the batch holds, as best the TV knows. */
export function arrivalCount(a: Arrival): number {
  return Math.max(a.count, a.expected ?? 0);
}

/**
 * Family channel: a phone's photos, full screen, the moment they can be shown. Takes over
 * the idle slideshow and Memories home, never a show someone chose. One button tells the
 * sender their photos were loved; the screen goes back where it was after a quiet spell.
 */
export function Arrived({
  api,
  token,
  arrival,
  onPlayAll,
  onClose,
  intervalMs = 5000,
  idleCloseMs = 90_000,
}: {
  api: TvApi;
  token: string;
  arrival: Arrival;
  onPlayAll: (media: string[], from: string) => void;
  onClose: () => void;
  intervalMs?: number;
  idleCloseMs?: number;
}) {
  const [index, setIndex] = useState(0);
  const [loved, setLoved] = useState<"no" | "sending" | "yes" | "failed">("no");
  const media = arrival.media;
  const count = arrivalCount(arrival);
  const current = media[Math.min(index, media.length - 1)];

  // A new batch starts over; more of the same batch just joins the rotation.
  const batch = useRef(arrival.batch);
  useEffect(() => {
    if (batch.current !== arrival.batch) {
      batch.current = arrival.batch;
      setIndex(0);
      setLoved("no");
    }
  }, [arrival.batch]);

  useEffect(() => {
    if (media.length < 2) return;
    const t = setInterval(() => setIndex((i) => (i + 1) % media.length), intervalMs);
    return () => clearInterval(t);
  }, [media.length, intervalMs]);

  // Back to where the TV was once nobody has touched it for a while; new photos restart
  // the wait.
  const [touched, setTouched] = useState(0);
  useEffect(() => {
    if (idleCloseMs <= 0) return;
    const t = setTimeout(onClose, idleCloseMs);
    return () => clearTimeout(t);
  }, [onClose, idleCloseMs, touched, arrival.seq]);
  useRemoteKeys({ onBack: onClose });

  const love = useCallback(async () => {
    setTouched((n) => n + 1);
    setLoved("sending");
    try {
      await api.loveArrival(token, { contributor: arrival.contributor, count, ...(media[0] ? { cover: media[0] } : {}) });
      setLoved("yes");
    } catch {
      setLoved("failed");
    }
  }, [api, token, arrival.contributor, count, media]);

  const noun = count === 1 ? "photo" : "photos";
  const loveLabel = { no: "Love it", sending: "Sending…", yes: `Loved · ${arrival.from} will see it`, failed: "Try again" }[loved];
  const shown = Math.min(index, media.length - 1) + 1;
  return (
    <View testID="arrived" style={{ flex: 1, backgroundColor: "#000" }}>
      {current && (
        <KenBurns slideKey={current} paused={false} durationMs={intervalMs}>
          <RemoteImage api={api} token={token} mediaId={current} width={1920} height={1080} radius={0} />
        </KenBurns>
      )}
      <View style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: 520, backgroundColor: "#000000b3" }} />
      <View style={{ position: "absolute", left: theme.safe.x, right: theme.safe.x, bottom: theme.safe.y + 20, gap: 18 }}>
        <Eyebrow testID="arrived-eyebrow">Just arrived</Eyebrow>
        <Title testID="arrived-title">{`From ${arrival.from}`}</Title>
        <Meta testID="arrived-meta" size={theme.body}>
          {media.length > 1 ? `${count} ${noun} · ${shown} of ${media.length} on screen` : `${count} ${noun}`}
        </Meta>
        <View style={{ flexDirection: "row", gap: 18, marginTop: 12 }}>
          <FocusButton label={loveLabel} onPress={() => void love()} preferred disabled={loved === "sending" || loved === "yes"} testID="love" />
          <FocusButton
            label="Play all"
            variant="ghost"
            onPress={() => {
              setTouched((n) => n + 1);
              onPlayAll(media, arrival.from);
            }}
            testID="play-all"
          />
          <FocusButton label="Close" variant="ghost" onPress={onClose} testID="close" />
        </View>
      </View>
    </View>
  );
}
