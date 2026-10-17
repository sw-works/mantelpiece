import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Animated, Easing, Image, Text, View } from "react-native";
import { ApiError, type Reel as ReelPlan, type ReelText, type TvApi } from "../api/client";
import { CropMotion, TV_FRAME } from "../components/CropMotion";
import { FocusButton } from "../components/FocusButton";
import { Eyebrow, Meta, Title } from "../components/Type";
import { useRemoteKeys } from "../hooks/useRemoteKeys";
import { type MusicPlayer } from "../music";
import { theme } from "../theme";

/** How often the reel reads its clock. */
export const TICK_MS = 100;
/** Overlays fade in and out, and each slide fades in over the one before, this fast. */
export const FADE_MS = 300;
/** Slides past the current one whose pictures are fetched ahead, so a cut never waits. */
export const PRELOAD_AHEAD = 2;
/** The music gets this long to load before the reel plays silently. */
export const MUSIC_TIMEOUT_MS = 10_000;
/** Music whose position stops moving this long no longer drives the reel. */
export const STALL_MS = 3_000;

type Phase = "loading" | "too-few" | "missing" | "starting" | "playing" | "ended";

/** What is on screen at a moment of the reel. */
export interface Showing {
  /** The slide whose [start_ms, end_ms) holds the moment. */
  index: number;
  /** The slide before, still under the current one while it fades in. */
  prev: number | null;
  title: boolean;
  label: boolean;
  credit: boolean;
}

/**
 * What is on screen at `ms`. `linger` keeps an overlay mounted that long past its end, so a
 * smoothed fade finishes before it goes.
 */
export function showingAt(reel: ReelPlan, ms: number, linger = 0): Showing {
  const slides = reel.slides;
  const found = slides.findIndex((s) => ms < s.end_ms);
  const index = found < 0 ? Math.max(0, slides.length - 1) : found;
  const current = slides[index];
  const prev = index > 0 && current && ms < current.start_ms + FADE_MS + linger ? index - 1 : null;
  const within = (t: ReelText | null | undefined) => Boolean(t && t.end_ms > t.start_ms && ms >= t.start_ms && ms < t.end_ms + linger);
  return { index, prev, title: within(reel.title), label: within(current?.label), credit: within(reel.credit) };
}

const same = (a: Showing, b: Showing) => a.index === b.index && a.prev === b.prev && a.title === b.title && a.label === b.label && a.credit === b.credit;

function prefetch(url: string) {
  try {
    void Promise.resolve(Image.prefetch(url)).catch(() => undefined);
  } catch {
    // No prefetch on this platform: the picture loads when it is drawn.
  }
}

/** Fades in over FADE_MS from `start`, out over FADE_MS to `end`, from the reel clock. */
function windowFade(clock: Animated.Value, t: ReelText) {
  const f = Math.max(1, Math.min(FADE_MS, (t.end_ms - t.start_ms) / 2));
  return clock.interpolate({ inputRange: [t.start_ms, t.start_ms + f, t.end_ms - f, t.end_ms], outputRange: [0, 1, 1, 0], extrapolate: "clamp" });
}

/**
 * F-713: a trip or one of its days as a music-synced reel. Pictures change on the music's
 * bar lines, each with its own crop move; the title, each landmark and the music credit
 * show in their windows. The clock is the music's position when the platform has a player,
 * the wall clock otherwise (a silent reel). Pause holds the picture and the music together.
 */
export function Reel({
  api,
  token,
  tripId,
  day,
  music = null,
  onBack,
  tickMs = TICK_MS,
  smoothMs = tickMs,
  now = Date.now,
  musicTimeoutMs = MUSIC_TIMEOUT_MS,
  stallMs = STALL_MS,
}: {
  api: TvApi;
  token: string;
  tripId: string;
  /** A day reel; the whole trip without it. */
  day?: number;
  /** Platform music; absent means a silent reel on the wall clock. */
  music?: MusicPlayer | null;
  onBack: () => void;
  tickMs?: number;
  /** The picture eases between clock readings over this long; 0 jumps (tests). */
  smoothMs?: number;
  now?: () => number;
  musicTimeoutMs?: number;
  stallMs?: number;
}) {
  const [plan, setPlan] = useState<ReelPlan | null>(null);
  const [phase, setPhase] = useState<Phase>("loading");
  const [paused, setPaused] = useState(false);
  const [showing, setShowing] = useState<Showing>({ index: 0, prev: null, title: false, label: false, credit: false });
  const [urls, setUrls] = useState<Record<string, string>>({});
  // The reel clock in milliseconds; every move and fade on screen is read from it.
  const clock = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    let alive = true;
    setPhase("loading");
    api
      .reel(token, tripId, day)
      .then((r) => {
        if (!alive) return;
        if (r.slides.length === 0) return setPhase("too-few");
        setPlan(r);
        setPhase("starting");
      })
      .catch((e: unknown) => alive && setPhase(e instanceof ApiError && e.status === 422 ? "too-few" : "missing"));
    return () => {
      alive = false;
    };
  }, [api, token, tripId, day]);

  // Clock state lives in refs: the timer reads it ten times a second without re-rendering.
  const phaseRef = useRef(phase);
  phaseRef.current = phase;
  const nowRef = useRef(now);
  nowRef.current = now;
  const pausedRef = useRef(false);
  /** The music was started and follows Pause and Play. */
  const musicStarted = useRef(false);
  /** The music's position is trusted as the clock. */
  const musicClock = useRef(false);
  /** The wall clock: `base` ms of reel at `since`, moving unless paused. */
  const wall = useRef({ base: 0, since: 0 });
  const lastPos = useRef({ value: -1, at: 0 });
  const shown = useRef(0);

  const readClock = useCallback((): number => {
    const t = nowRef.current();
    const raw = musicClock.current && music ? music.positionMs() : null;
    let pos = raw !== null && Number.isFinite(raw) ? raw : null;
    if (pos !== null && !pausedRef.current) {
      if (pos !== lastPos.current.value) lastPos.current = { value: pos, at: t };
      else if (t - lastPos.current.at > stallMs) {
        // The music stopped moving: the reel carries on silently from where it got to.
        musicClock.current = false;
        pos = null;
      }
    }
    if (pos !== null) {
      wall.current = { base: pos, since: t };
      return pos;
    }
    return wall.current.base + (pausedRef.current ? 0 : t - wall.current.since);
  }, [music, stallMs]);

  const drive = useCallback(
    (ms: number) => {
      const step = ms - shown.current;
      shown.current = ms;
      if (step === 0) return;
      // Small steps forward ease over the tick so the move looks continuous; anything else jumps.
      if (smoothMs <= 0 || step < 0 || step > smoothMs * 4) clock.setValue(ms);
      else Animated.timing(clock, { toValue: ms, duration: smoothMs, easing: Easing.linear, useNativeDriver: false }).start();
    },
    [clock, smoothMs],
  );

  // Starting (and Play again): load the music, then start it and the clock together.
  useEffect(() => {
    if (phase !== "starting" || !plan) return;
    let alive = true;
    const begin = (withMusic: boolean) => {
      if (!alive) return;
      const t = nowRef.current();
      musicStarted.current = withMusic;
      musicClock.current = withMusic;
      wall.current = { base: 0, since: t };
      lastPos.current = { value: -1, at: t };
      pausedRef.current = false;
      setPaused(false);
      shown.current = 0;
      clock.setValue(0);
      setShowing(showingAt(plan, 0, smoothMs));
      if (withMusic) music?.play();
      setPhase("playing");
    };
    if (!music) {
      begin(false);
      return;
    }
    let settled = false;
    const settle = (ok: boolean) => {
      if (settled || !alive) return;
      settled = true;
      clearTimeout(timer);
      if (!ok) music.stop();
      begin(ok);
    };
    const timer = setTimeout(() => settle(false), musicTimeoutMs);
    music.load(plan.track.url).then(
      () => settle(true),
      () => settle(false),
    );
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [phase, plan, music, musicTimeoutMs, clock, smoothMs]);

  useEffect(() => {
    if (phase !== "playing" || !plan) return;
    const t = setInterval(() => {
      const ms = readClock();
      drive(ms);
      const next = showingAt(plan, ms, smoothMs);
      setShowing((s) => (same(s, next) ? s : next));
      if (ms >= plan.duration_ms) {
        music?.stop();
        musicStarted.current = false;
        setPhase("ended");
      }
    }, tickMs);
    return () => clearInterval(t);
  }, [phase, plan, readClock, drive, tickMs, smoothMs, music]);

  // Leaving, by Back or by anything else taking the screen, stops the music.
  useEffect(() => () => music?.stop(), [music]);

  const togglePause = useCallback(() => {
    if (phaseRef.current !== "playing") return;
    const t = nowRef.current();
    if (!pausedRef.current) {
      const ms = readClock();
      wall.current = { base: ms, since: t };
      pausedRef.current = true;
      if (musicStarted.current) music?.pause();
    } else {
      wall.current = { ...wall.current, since: t };
      lastPos.current = { ...lastPos.current, at: t };
      pausedRef.current = false;
      if (musicStarted.current) music?.play();
    }
    setPaused(pausedRef.current);
  }, [music, readClock]);

  const leave = useCallback(() => {
    music?.stop();
    onBack();
  }, [music, onBack]);
  useRemoteKeys({ onBack: leave, onPlayPause: togglePause });

  // The current picture and the next ones are resolved and fetched ahead of their cuts.
  const mounted = useRef(true);
  useEffect(
    () => () => {
      mounted.current = false;
    },
    [],
  );
  const requested = useRef(new Set<string>());
  useEffect(() => {
    if (!plan) return;
    for (const s of plan.slides.slice(showing.index, showing.index + 1 + PRELOAD_AHEAD)) {
      if (requested.current.has(s.media)) continue;
      requested.current.add(s.media);
      api
        .mediaUrl(token, s.media, "tv")
        .then((r) => {
          prefetch(r.url);
          if (mounted.current) setUrls((u) => ({ ...u, [s.media]: r.url }));
        })
        // Asked again at the next cut.
        .catch(() => requested.current.delete(s.media));
    }
  }, [api, token, plan, showing.index]);

  const fades = useMemo(() => {
    if (!plan) return null;
    return {
      slideIn: plan.slides.map((s) => clock.interpolate({ inputRange: [s.start_ms, s.start_ms + FADE_MS], outputRange: [0, 1], extrapolate: "clamp" })),
      label: plan.slides.map((s) => (s.label ? windowFade(clock, s.label) : null)),
      title: windowFade(clock, plan.title),
      credit: windowFade(clock, plan.credit),
      out: clock.interpolate({ inputRange: [plan.duration_ms - Math.max(1, plan.fade_out_ms), plan.duration_ms], outputRange: [0, 1], extrapolate: "clamp" }),
    };
  }, [plan, clock]);

  if (phase === "loading" || phase === "starting" || !plan || !fades) {
    if (phase === "too-few") return <Notice testID="reel-too-few" title="Not enough photos for a reel" line="A reel needs at least three photos from the day." onBack={leave} />;
    if (phase === "missing") return <Notice testID="reel-missing" title="This reel could not be made" line="Try again in a moment." onBack={leave} />;
    return <Notice testID="reel-loading" line="Loading the reel…" onBack={leave} />;
  }
  if (phase === "ended") {
    return (
      <View testID="reel-ended" style={{ flex: 1, backgroundColor: "#000", paddingHorizontal: theme.safe.x, justifyContent: "center", gap: theme.gap }}>
        {plan.title.sub ? <Eyebrow>{plan.title.sub}</Eyebrow> : null}
        <Title size={theme.display}>{plan.title.text}</Title>
        <Meta testID="reel-ended-credit">{plan.credit.text}</Meta>
        <View style={{ flexDirection: "row", gap: 14, marginTop: 8 }}>
          <FocusButton label="Play again" preferred testID="play-again" onPress={() => setPhase("starting")} />
          <FocusButton label="Back" variant="ghost" testID="back" onPress={leave} />
        </View>
      </View>
    );
  }

  const current = plan.slides[showing.index];
  const layers = showing.prev !== null ? [showing.prev, showing.index] : [showing.index];
  const labelFade = fades.label[showing.index];
  return (
    <View testID="reel" style={{ flex: 1, backgroundColor: "#000" }}>
      {layers.map((i) => {
        const s = plan.slides[i]!;
        const top = i === showing.index;
        return (
          <Animated.View key={`slide-${i}`} testID={top ? "reel-current" : "reel-previous"} pointerEvents="none" style={{ position: "absolute", left: 0, top: 0, width: TV_FRAME.w, height: TV_FRAME.h, opacity: top ? fades.slideIn[i]! : 1 }}>
            <CropMotion uri={urls[s.media] ?? null} slide={s} clock={clock} testID={top ? "crop-current" : undefined} />
          </Animated.View>
        );
      })}
      {showing.title && (
        <Animated.View testID="reel-title" pointerEvents="none" style={{ position: "absolute", left: 0, top: 0, width: TV_FRAME.w, height: TV_FRAME.h, alignItems: "center", justifyContent: "center", opacity: fades.title }}>
          <View style={{ alignItems: "center", gap: 10, maxWidth: TV_FRAME.w - 2 * theme.safe.x, paddingHorizontal: 56, paddingVertical: 32, borderRadius: 32, backgroundColor: "#0b0b0f8c" }}>
            {plan.title.sub ? <Eyebrow testID="reel-title-sub">{plan.title.sub}</Eyebrow> : null}
            <Title size={theme.display} testID="reel-title-text">
              {plan.title.text}
            </Title>
          </View>
        </Animated.View>
      )}
      {showing.label && current?.label && labelFade && (
        <Animated.View key={`label-${showing.index}`} testID="reel-label" pointerEvents="none" style={{ position: "absolute", left: theme.safe.x, bottom: theme.safe.y, maxWidth: 1000, paddingHorizontal: 24, paddingVertical: 12, borderRadius: 24, backgroundColor: "#0b0b0fb3", opacity: labelFade }}>
          <Text testID="reel-label-text" style={{ color: theme.fg, fontSize: theme.heading, fontWeight: "600" }}>
            {current.label.text}
          </Text>
        </Animated.View>
      )}
      {showing.credit && (
        <Animated.View testID="reel-credit" pointerEvents="none" style={{ position: "absolute", right: theme.safe.x, bottom: theme.safe.y, maxWidth: 1100, paddingHorizontal: 20, paddingVertical: 10, borderRadius: 20, backgroundColor: "#0b0b0fb3", opacity: fades.credit }}>
          <Text testID="reel-credit-text" style={{ color: theme.fg, fontSize: theme.small }}>
            {plan.credit.text}
          </Text>
        </Animated.View>
      )}
      <Animated.View testID="reel-fade" pointerEvents="none" style={{ position: "absolute", left: 0, top: 0, width: TV_FRAME.w, height: TV_FRAME.h, backgroundColor: "#000", opacity: fades.out }} />
      {/* Out of sight while playing but still focused, so Select pauses; Pause brings it up. */}
      <View testID="reel-controls" style={{ position: "absolute", top: theme.safe.y, right: theme.safe.x, flexDirection: "row", gap: 12, alignItems: "center", opacity: paused ? 1 : 0 }}>
        {paused && (
          <View style={{ paddingHorizontal: 20, height: 56, justifyContent: "center", borderRadius: 28, backgroundColor: "#0b0b0fd9" }}>
            <Meta testID="reel-paused" color={theme.fg}>
              Paused
            </Meta>
          </View>
        )}
        <FocusButton label={paused ? "Play" : "Pause"} onPress={togglePause} preferred testID="reel-playpause" />
        <FocusButton label="Back" variant="ghost" onPress={leave} testID="back" />
      </View>
    </View>
  );
}

function Notice({ testID, title, line, onBack }: { testID: string; title?: string; line: string; onBack: () => void }) {
  return (
    <View testID={testID} style={{ flex: 1, backgroundColor: "#000", paddingHorizontal: theme.safe.x, justifyContent: "center", gap: theme.gap }}>
      <Eyebrow>Reel</Eyebrow>
      {title ? <Title>{title}</Title> : null}
      <Meta size={theme.body}>{line}</Meta>
      <FocusButton label="Back" onPress={onBack} preferred testID="back" />
    </View>
  );
}
