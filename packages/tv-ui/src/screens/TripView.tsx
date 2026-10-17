import { useEffect, useMemo, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { clockHints, creditFor, dayFacts, dayLabel, type FilmInfo, longDate, type ReactionsView, storyOf, type Trip, type TvApi, withPeople } from "../api/client";
import { Backdrop } from "../components/Backdrop";
import { FocusButton } from "../components/FocusButton";
import { LandmarkCard } from "../components/LandmarkCard";
import { MapImage } from "../components/MapImage";
import { PhotoCard } from "../components/PhotoCard";
import { type Nav, Screen } from "../components/Screen";
import { Eyebrow, Heading, Meta, Title } from "../components/Type";
import { theme } from "../theme";

/** Names of the people who took a day's photos, in first-appearance order (F-154). */
export function dayCredits(trip: Trip, media: string[]): string[] {
  const names: string[] = [];
  for (const m of media) {
    const name = creditFor(trip, m);
    if (name && !names.includes(name)) names.push(name);
  }
  return names;
}

/** A reel or film needs this many photos on the day (contracts/api.md: fewer answers 422). */
export const MIN_REEL_PHOTOS = 3;

/**
 * F-403: one trip. Its photo behind the title, the days as a row of chips, the chosen
 * day's photos with credits and reactions, its route, what its landmarks are (F-210) and
 * its story beside them. With face grouping on, a row of people narrows the days, the
 * photos and Play to the photos they are in (F-704). The trip and each day play as a reel
 * (F-713); a day with a film plays it, and the owner can have one made now (F-714).
 */
export function TripView({
  api,
  token,
  tripId,
  onBack,
  onInvite,
  onHighlights,
  onPlay,
  onReel,
  onFilm,
  canManage = true,
  reactionsVersion = 0,
  filmsVersion = 0,
  nav,
}: {
  api: TvApi;
  token: string;
  tripId: string;
  onBack: () => void;
  onInvite: (tripId: string) => void;
  onHighlights?: (tripId: string) => void;
  onPlay?: (trip: Trip) => void;
  /** The whole trip's reel, or one day's (F-713). */
  onReel?: (trip: Trip, day?: number) => void;
  /** A day's film (F-714); without it the TV offers no films. */
  onFilm?: (trip: Trip, day: number) => void;
  /** False on a joined trip: no promote, no rewrite, no film making (F-165). */
  canManage?: boolean;
  /** Bumped by the feed on every reaction so counts refresh (F-710). */
  reactionsVersion?: number;
  /** Bumped by the feed when a day film is ready, so the film button appears. */
  filmsVersion?: number;
  nav?: Nav;
}) {
  const [reactions, setReactions] = useState<ReactionsView | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .reactions(token, tripId)
      .then((r) => alive && setReactions(r))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [api, token, tripId, reactionsVersion]);
  const summaryFor = (key: string) => reactions?.summary[key];
  const likesLine = (key: string) => {
    const s = summaryFor(key);
    if (!s || (s.likes === 0 && s.comments === 0)) return "";
    return `${s.likes ? `❤ ${s.likes}` : ""}${s.likes && s.comments ? " · " : ""}${s.comments ? `${s.comments} comment${s.comments === 1 ? "" : "s"}` : ""}`;
  };
  const [full, setTrip] = useState<Trip | null>(null);
  const [only, setOnly] = useState<string[]>([]);
  const [day, setDay] = useState(0);
  const [showDayMap, setShowDayMap] = useState(false);
  const [rewriting, setRewriting] = useState(false);
  const [roles, setRoles] = useState<Record<string, string>>({});
  useEffect(() => {
    let alive = true;
    api
      .trip(token, tripId)
      .then((t) => alive && setTrip(t))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [api, token, tripId]);

  // F-714: which days have a film, asked for the chosen day and again when one is ready.
  const [films, setFilms] = useState<Record<number, FilmInfo | null>>({});
  const [filmAsks, setFilmAsks] = useState<Record<number, "queued" | "failed">>({});
  const filmsOn = Boolean(onFilm);
  const chosen = useMemo(() => {
    if (!full) return null;
    const days = withPeople(full, only).days;
    const d = days[Math.min(day, days.length - 1)];
    // The reel and the film are of the whole day, whoever is picked above.
    const photos = d ? (full.days.find((x) => x.index === d.index)?.media.length ?? 0) : 0;
    return d ? { index: d.index, reelable: photos >= MIN_REEL_PHOTOS } : null;
  }, [full, only, day]);
  const filmDay = chosen?.reelable ? chosen.index : null;
  useEffect(() => {
    if (!filmsOn || filmDay === null) return;
    let alive = true;
    api
      .film(token, tripId, filmDay)
      .then((f) => alive && setFilms((s) => ({ ...s, [filmDay]: f })))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [api, token, tripId, filmDay, filmsOn, filmsVersion]);
  // A film landed: "will be ready" lines give way to the film button.
  useEffect(() => setFilmAsks({}), [filmsVersion]);
  const makeFilm = (index: number) => {
    api
      .makeFilm(token, tripId, index)
      .then(() => setFilmAsks((s) => ({ ...s, [index]: "queued" })))
      .catch(() => setFilmAsks((s) => ({ ...s, [index]: "failed" })));
  };

  if (!full) return <Screen nav={nav} active="home" />;
  const trip = withPeople(full, only);
  const named = (full.people ?? []).filter((p) => p.name);
  const togglePerson = (id: string) => {
    setOnly((s) => (s.includes(id) ? s.filter((x) => x !== id) : [...s, id]));
    setDay(0);
  };
  const current = trip.days[Math.min(day, trip.days.length - 1)];
  const hasRoute = (current?.route.points.length ?? 0) > 0;
  const facts = current ? dayFacts(trip, current) : [];
  const credits = current ? dayCredits(trip, current.media) : [];
  const hints = clockHints(trip);
  const story = storyOf(trip);
  const roleOf = (id: string) => roles[id] ?? trip.contributor_roles?.[id] ?? "contributor";
  const people = Object.entries(trip.contributor_names ?? {});
  const span = `${longDate(trip.start)} to ${longDate(trip.end)}`;
  return (
    <Screen nav={nav} active="home" backdrop={<Backdrop api={api} token={token} mediaId={trip.hero} height={520} dim={0.7} />}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "flex-start", gap: 24 }}>
        <View style={{ gap: 12, flex: 1 }}>
          <Eyebrow>{span}</Eyebrow>
          <Title size={84}>{trip.name}</Title>
          {trip.shared_by && <Text testID="shared-by" style={{ color: theme.accent, fontSize: theme.small }}>{`Shared by ${trip.shared_by}`}</Text>}
        </View>
        <View style={{ flexDirection: "row", gap: 12, marginTop: 8 }}>
          {onPlay && <FocusButton label="Play" onPress={() => onPlay(trip)} testID="play" />}
          {onReel && <FocusButton label="Play reel" variant="ghost" onPress={() => onReel(full)} testID="play-reel" />}
          {onHighlights && <FocusButton label="Highlights" variant="ghost" onPress={() => onHighlights(trip.id)} testID="highlights" />}
          <FocusButton label="Invite" variant="ghost" onPress={() => onInvite(trip.id)} testID="invite" />
          <FocusButton label="Back" variant="ghost" onPress={onBack} testID="back" />
        </View>
      </View>
      {named.length > 0 && (
        <View testID="people-filter" style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
          <Meta>Only photos with</Meta>
          <FocusButton variant="chip" selected={only.length === 0} label="Everyone" onPress={() => setOnly([])} testID="people-everyone" />
          {named.slice(0, 8).map((p) => (
            <FocusButton key={p.id} variant="chip" selected={only.includes(p.id)} label={`${p.name} · ${p.media_count}`} onPress={() => togglePerson(p.id)} testID={`only-${p.id}`} />
          ))}
        </View>
      )}
      <ScrollView horizontal style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 14, paddingVertical: 6 }}>
        {trip.days.map((d, i) => (
          <FocusButton key={d.index} variant="chip" selected={i === day} label={dayLabel(d)} preferred={i === 0} onPress={() => setDay(i)} testID={`day-${d.index}`} />
        ))}
      </ScrollView>
      {!current && only.length > 0 && <Meta testID="no-photos-together">No photos on this trip have all of them together.</Meta>}
      {current && (
        <View style={{ flex: 1, flexDirection: "row", gap: 40 }}>
          <View style={{ width: 1000, gap: 18 }}>
            <View style={{ flexDirection: "row", gap: 16, alignItems: "baseline" }}>
              <Heading>{dayLabel(current)}</Heading>
              <Meta testID="day-summary">
                {`${longDate(current.date)} · ${current.media.length} photo${current.media.length === 1 ? "" : "s"}${credits.length ? ` from ${credits.join(", ")}` : ""}${current.route.inferred ? " · route inferred" : ""}${current.undated?.length ? ` · ${current.undated.length} undated` : ""}`}
              </Meta>
            </View>
            {chosen?.reelable && (onReel || onFilm) && (
              <View testID="day-actions" style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
                {onReel && <FocusButton variant="chip" label="Day reel" testID={`day-reel-${current.index}`} onPress={() => onReel(full, current.index)} />}
                {onFilm && films[current.index] && <FocusButton variant="chip" label="Day film" testID={`day-film-${current.index}`} onPress={() => onFilm(full, current.index)} />}
                {onFilm && canManage && filmAsks[current.index] !== "queued" && <FocusButton variant="chip" label="Make day film" testID={`make-film-${current.index}`} onPress={() => makeFilm(current.index)} />}
                {filmAsks[current.index] === "queued" && <Meta testID="film-queued">Your film will be ready in a few minutes</Meta>}
                {filmAsks[current.index] === "failed" && <Meta testID="film-failed">The film could not be started. Try again later.</Meta>}
              </View>
            )}
            <ScrollView horizontal style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 18, padding: 6, paddingBottom: 12 }}>
              {current.media.slice(0, 12).map((m) => (
                <View key={m} style={{ gap: 6, width: 250 }}>
                  <PhotoCard api={api} token={token} mediaId={m} width={250} height={168} />
                  <Text testID={`credit-${m}`} style={{ color: theme.muted, fontSize: theme.credit }}>
                    {`${creditFor(trip, m) ?? ""}${likesLine(`media:${m}`) ? ` · ${likesLine(`media:${m}`)}` : ""}`}
                  </Text>
                  {summaryFor(`media:${m}`)?.latest_comment && (
                    <Text testID={`comment-${m}`} numberOfLines={2} style={{ color: theme.fg, fontSize: theme.credit }}>
                      {`${summaryFor(`media:${m}`)!.latest_comment!.by}: ${summaryFor(`media:${m}`)!.latest_comment!.text}`}
                    </Text>
                  )}
                </View>
              ))}
            </ScrollView>
            {likesLine(`day:${current.index}`) !== "" && (
              <Text testID="day-reactions" style={{ color: theme.accent, fontSize: theme.small }}>
                {`${likesLine(`day:${current.index}`)}${summaryFor(`day:${current.index}`)?.latest_comment ? ` · ${summaryFor(`day:${current.index}`)!.latest_comment!.by}: ${summaryFor(`day:${current.index}`)!.latest_comment!.text}` : ""}`}
              </Text>
            )}
            {hints.length > 0 && (
              <Text testID="clock-hints" style={{ color: theme.muted, fontSize: theme.credit }}>
                {hints.join(" · ")}
              </Text>
            )}
            {story && !rewriting ? (
              <View style={{ gap: 12 }}>
                <Text testID="narrative" style={{ color: theme.fg, fontSize: theme.heading, lineHeight: 42, maxWidth: 1000 }}>
                  {current.story ?? story}
                </Text>
                <View style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
                  {canManage && (
                    <FocusButton
                      variant="chip"
                      label="Rewrite the story"
                      testID="regenerate"
                      onPress={() => {
                        setRewriting(true);
                        api.regenerateNarrative(token, trip.id).catch(() => setRewriting(false));
                      }}
                    />
                  )}
                  <Meta size={theme.credit}>Written by Mantelpiece from the day's photos</Meta>
                </View>
              </View>
            ) : (
              <Meta testID="writing">The story of this trip is being written.</Meta>
            )}
          </View>
          <View style={{ flex: 1, gap: 16 }}>
            {/* F-712: the whole trip's sketch map, or the chosen day's. */}
            <MapImage api={api} token={token} tripId={trip.id} day={showDayMap && hasRoute ? current.index : undefined} width={440} height={248} />
            {hasRoute && (
              <FocusButton
                variant="chip"
                label={showDayMap ? "Whole trip map" : `Map of day ${current.index}`}
                testID="map-toggle"
                onPress={() => setShowDayMap((v) => !v)}
              />
            )}
            {facts.slice(0, 2).map((f) => (
              <LandmarkCard key={f.key} facts={f} lines={1} width={440} />
            ))}
            {people.length > 0 && (
              <View testID="contributors" style={{ gap: 8, marginTop: 8 }}>
                <Text style={{ color: theme.fg, fontSize: theme.small, fontWeight: "600" }}>On this trip</Text>
                {people.map(([id, name]) => (
                  <View key={id} style={{ flexDirection: "row", gap: 12, alignItems: "center" }}>
                    <Meta>{`${name} · ${roleOf(id).replace("_", "-")}`}</Meta>
                    {canManage && roleOf(id) === "contributor" && (
                      <FocusButton
                        variant="chip"
                        label="Promote to co-editor"
                        testID={`promote-${id}`}
                        onPress={() => {
                          api
                            .setRole(token, trip.id, id, "co_editor")
                            .then((r) => setRoles((s) => ({ ...s, [id]: r.role })))
                            .catch(() => undefined);
                        }}
                      />
                    )}
                  </View>
                ))}
              </View>
            )}
          </View>
        </View>
      )}
    </Screen>
  );
}
