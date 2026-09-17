import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { clockHints, creditFor, type ReactionsView, storyOf, type Trip, type TvApi } from "../api/client";
import { Backdrop } from "../components/Backdrop";
import { FocusButton } from "../components/FocusButton";
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

/**
 * F-403: one trip. Its photo behind the title, the days as a row of chips, the chosen
 * day's photos with credits and reactions, its route and its story beside them.
 */
export function TripView({
  api,
  token,
  tripId,
  onBack,
  onInvite,
  onHighlights,
  onPlay,
  canManage = true,
  reactionsVersion = 0,
  nav,
}: {
  api: TvApi;
  token: string;
  tripId: string;
  onBack: () => void;
  onInvite: (tripId: string) => void;
  onHighlights?: (tripId: string) => void;
  onPlay?: (trip: Trip) => void;
  /** False on a joined trip: no promote, no rewrite (F-165). */
  canManage?: boolean;
  /** Bumped by the feed on every reaction so counts refresh (F-710). */
  reactionsVersion?: number;
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
  const [trip, setTrip] = useState<Trip | null>(null);
  const [day, setDay] = useState(0);
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

  if (!trip) return <Screen nav={nav} active="home" />;
  const current = trip.days[day];
  const credits = current ? dayCredits(trip, current.media) : [];
  const hints = clockHints(trip);
  const story = storyOf(trip);
  const roleOf = (id: string) => roles[id] ?? trip.contributor_roles?.[id] ?? "contributor";
  const people = Object.entries(trip.contributor_names ?? {});
  const span = `${trip.start.slice(0, 10)} to ${trip.end.slice(0, 10)}`;
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
          {onHighlights && <FocusButton label="Highlights" variant="ghost" onPress={() => onHighlights(trip.id)} testID="highlights" />}
          <FocusButton label="Invite" variant="ghost" onPress={() => onInvite(trip.id)} testID="invite" />
          <FocusButton label="Back" variant="ghost" onPress={onBack} testID="back" />
        </View>
      </View>
      <ScrollView horizontal style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 14, paddingVertical: 6 }}>
        {trip.days.map((d, i) => (
          <FocusButton key={d.index} variant="chip" selected={i === day} label={`Day ${d.index}${d.title ? ` · ${d.title}` : ""}`} preferred={i === 0} onPress={() => setDay(i)} testID={`day-${d.index}`} />
        ))}
      </ScrollView>
      {current && (
        <View style={{ flex: 1, flexDirection: "row", gap: 40 }}>
          <View style={{ width: 1000, gap: 18 }}>
            <View style={{ flexDirection: "row", gap: 16, alignItems: "baseline" }}>
              <Heading>{current.title ? `Day ${current.index} · ${current.title}` : `Day ${current.index}`}</Heading>
              <Meta testID="day-summary">
                {`${current.date} · ${current.media.length} photos${credits.length ? ` from ${credits.join(", ")}` : ""}${current.route.inferred ? " · route inferred" : ""}${current.undated?.length ? ` · ${current.undated.length} undated` : ""}`}
              </Meta>
            </View>
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
                  <Meta size={theme.credit}>Written by MemoryLane from the day's photos</Meta>
                </View>
              </View>
            ) : (
              <Meta testID="writing">The story of this trip is being written.</Meta>
            )}
          </View>
          <View style={{ flex: 1, gap: 16 }}>
            <MapImage api={api} token={token} tripId={trip.id} day={current.route.map_key ? current.index : undefined} width={440} height={248} />
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
