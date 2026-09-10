import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { type TripSummary, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { theme } from "../theme";

/** F-402: trips as rows, hero from the top-scored photo, new-memory badge. */
export function MemoriesHome({
  api,
  token,
  tripsVersion,
  onOpenTrip,
  onInvite,
  onAsk,
  onReview,
  pendingReviews = 0,
}: {
  api: TvApi;
  token: string;
  tripsVersion: number;
  onOpenTrip: (trip: TripSummary) => void;
  onInvite: () => void;
  onAsk?: () => void;
  onReview?: () => void;
  pendingReviews?: number;
}) {
  const [trips, setTrips] = useState<TripSummary[] | null>(null);
  const [seen, setSeen] = useState<Set<string>>(new Set());
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
      <ScrollView>
        {trips?.map((trip, i) => (
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
            </View>
            <FocusButton
              label="Open"
              preferred={i === 0}
              onPress={() => {
                setSeen(new Set([...seen, trip.id]));
                onOpenTrip(trip);
              }}
            />
          </View>
        ))}
      </ScrollView>
    </View>
  );
}
