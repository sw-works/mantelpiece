import { useEffect, useRef, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { playlistAsTrip, type MemoriesHomeView, type Trip, type TripSummary, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { theme } from "../theme";
import { groupByYear } from "../trips";

/**
 * F-402: trips as rows under a heading per year, hero from the top-scored photo,
 * new-memory badge. With more than one year a jump row scrolls to that year's heading.
 */
export function MemoriesHome({
  api,
  token,
  tripsVersion,
  onOpenTrip,
  onInvite,
  onAsk,
  onReview,
  pendingReviews = 0,
  onJoin,
  onLeave,
  onMap,
  onPlay,
  onPeople,
}: {
  api: TvApi;
  token: string;
  tripsVersion: number;
  onOpenTrip: (trip: TripSummary) => void;
  onInvite: () => void;
  onAsk?: () => void;
  onReview?: () => void;
  pendingReviews?: number;
  onJoin?: () => void;
  onLeave?: (trip: TripSummary) => Promise<void> | void;
  onMap?: () => void;
  onPlay?: (trip: Trip) => void;
  onPeople?: () => void;
}) {
  const [trips, setTrips] = useState<TripSummary[] | null>(null);
  const [memories, setMemories] = useState<MemoriesHomeView | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .memoriesHome(token)
      .then((m) => alive && setMemories(m))
      .catch(() => alive && setMemories(null));
    return () => {
      alive = false;
    };
  }, [api, token, tripsVersion]);
  const [seen, setSeen] = useState<Set<string>>(new Set());
  const scroller = useRef<ScrollView>(null);
  const yearTop = useRef<Record<string, number>>({});
  const years = groupByYear(trips ?? []);
  useEffect(() => {
    let alive = true;
    api
      .trips(token)
      .then((t) => alive && setTrips(t))
      .catch(() => alive && setTrips([]));
    return () => {
      alive = false;
    };
  }, [api, token, tripsVersion]);

  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: theme.gap }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>Memories</Text>
        <View style={{ flexDirection: "row", gap: 16 }}>
          {onReview && (pendingReviews > 0 || true) && (
            <FocusButton label={pendingReviews > 0 ? `Review changes (${pendingReviews})` : "Review changes"} onPress={onReview} testID="review" />
          )}
          {onPeople && <FocusButton label="People" onPress={onPeople} testID="people" />}
          {onMap && <FocusButton label="Map" onPress={onMap} testID="map" />}
          {onJoin && <FocusButton label="Join a trip" onPress={onJoin} testID="join" />}
          {onAsk && <FocusButton label="Ask" onPress={onAsk} testID="ask" />}
          <FocusButton label="Add photos" onPress={onInvite} testID="invite" />
        </View>
      </View>
      {trips === null && <Text style={{ color: theme.muted, fontSize: theme.body }}>Loading…</Text>}
      {trips !== null && trips.length === 0 && (
        <Text testID="empty" style={{ color: theme.muted, fontSize: theme.body }}>
          No trips yet. Press Add photos, scan the code with your phone, and pick a trip's worth of pictures.
        </Text>
      )}
      {years.length > 1 && (
        <View testID="year-jump" style={{ flexDirection: "row", gap: 12, flexWrap: "wrap" }}>
          {years.map((group) => (
            <FocusButton
              key={group.year}
              label={group.year}
              testID={`jump-${group.year}`}
              onPress={() => scroller.current?.scrollTo?.({ y: yearTop.current[group.year] ?? 0, animated: true })}
            />
          ))}
        </View>
      )}
      <ScrollView ref={scroller}>
        {memories && memories.on_this_day.length > 0 && (
          <View testID="on-this-day" style={{ gap: 8, marginBottom: theme.gap }}>
            <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
              <Text style={{ color: theme.fg, fontSize: 34, fontWeight: "600" }}>On this day</Text>
              {onPlay && <FocusButton label="Play" testID="play-on-this-day" onPress={() => onPlay(playlistAsTrip(`On this day, ${memories.date}`, memories.on_this_day))} />}
            </View>
            <ScrollView horizontal contentContainerStyle={{ gap: 16 }}>
              {memories.on_this_day.map((m) => (
                <RemoteImage key={m} api={api} token={token} mediaId={m} rendition="thumb" width={280} height={190} />
              ))}
            </ScrollView>
          </View>
        )}
        {memories && memories.then_and_now[0] && (
          <View testID="then-and-now" style={{ flexDirection: "row", gap: 16, alignItems: "center", marginBottom: theme.gap, backgroundColor: theme.panel, padding: 16, borderRadius: theme.radius }}>
            <RemoteImage api={api} token={token} mediaId={memories.then_and_now[0].then} rendition="thumb" width={280} height={190} />
            <RemoteImage api={api} token={token} mediaId={memories.then_and_now[0].now} rendition="thumb" width={280} height={190} />
            <View style={{ flex: 1, gap: 6 }}>
              <Text style={{ color: theme.fg, fontSize: 34, fontWeight: "600" }}>Then and now</Text>
              <Text testID="then-and-now-line" style={{ color: theme.muted, fontSize: theme.small }}>{`Same place, ${memories.then_and_now[0].years_apart} year${memories.then_and_now[0].years_apart === 1 ? "" : "s"} apart`}</Text>
            </View>
          </View>
        )}
        {years.map((group, g) => (
          <View key={group.year} testID={`year-${group.year}`} onLayout={(e) => (yearTop.current[group.year] = e.nativeEvent.layout.y)} style={{ gap: theme.gap }}>
            {years.length > 1 && (
              <Text testID={`year-heading-${group.year}`} style={{ color: theme.muted, fontSize: 30, fontWeight: "600", marginTop: g === 0 ? 0 : theme.gap }}>
                {group.year}
              </Text>
            )}
        {group.trips.map((trip, i) => (
          <View key={trip.id} testID={`trip-${trip.id}`} style={{ flexDirection: "row", gap: theme.gap, marginBottom: theme.gap, alignItems: "center" }}>
            {trip.hero && <RemoteImage api={api} token={token} mediaId={trip.hero} rendition="thumb" width={320} height={200} />}
            <View style={{ flex: 1, gap: 8 }}>
              <Text style={{ color: theme.fg, fontSize: 34, fontWeight: "600" }}>{trip.name}</Text>
              <Text style={{ color: theme.muted, fontSize: theme.small }}>
                {`${trip.start.slice(0, 10)} to ${trip.end.slice(0, 10)} · ${trip.contributors.length} contributor${trip.contributors.length === 1 ? "" : "s"}`}
              </Text>
              {!seen.has(trip.id) && (
                <Text testID={`new-${trip.id}`} style={{ color: theme.accent, fontSize: theme.small }}>New memory</Text>
              )}
              {trip.shared_by && (
                <Text testID={`shared-${trip.id}`} style={{ color: theme.accent, fontSize: theme.small }}>{`Shared by ${trip.shared_by}`}</Text>
              )}
            </View>
            {trip.shared_by && onLeave && (
              <FocusButton
                label="Leave"
                testID={`leave-${trip.id}`}
                onPress={() => {
                  void Promise.resolve(onLeave(trip)).then(() => setTrips((t) => (t ?? []).filter((x) => x.id !== trip.id)));
                }}
              />
            )}
            <FocusButton
              label="Open"
              preferred={g === 0 && i === 0}
              onPress={() => {
                setSeen(new Set([...seen, trip.id]));
                onOpenTrip(trip);
              }}
            />
          </View>
        ))}
          </View>
        ))}
      </ScrollView>
    </View>
  );
}
