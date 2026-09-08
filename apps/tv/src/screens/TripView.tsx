import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { theme } from "../theme";

/** F-403: day timeline, route map, narrative, highlights, Invite (F-409). */
export function TripView({
  api,
  token,
  tripId,
  onBack,
  onInvite,
}: {
  api: TvApi;
  token: string;
  tripId: string;
  onBack: () => void;
  onInvite: (tripId: string) => void;
}) {
  const [trip, setTrip] = useState<Trip | null>(null);
  const [day, setDay] = useState(0);
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

  if (!trip) return <View style={{ flex: 1, backgroundColor: theme.bg }} />;
  const current = trip.days[day];
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, padding: 64, gap: theme.gap }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>{trip.name}</Text>
        <View style={{ flexDirection: "row", gap: 16 }}>
          <FocusButton label="Invite" onPress={() => onInvite(trip.id)} testID="invite" />
          <FocusButton label="Back" onPress={onBack} testID="back" />
        </View>
      </View>
      <ScrollView horizontal contentContainerStyle={{ gap: 16 }}>
        {trip.days.map((d, i) => (
          <FocusButton key={d.index} label={d.title ?? `Day ${d.index}`} preferred={i === 0} onPress={() => setDay(i)} testID={`day-${d.index}`} />
        ))}
      </ScrollView>
      {current && (
        <View style={{ flexDirection: "row", gap: theme.gap }}>
          <ScrollView horizontal contentContainerStyle={{ gap: 16 }} style={{ flex: 2 }}>
            {current.media.slice(0, 12).map((m) => (
              <RemoteImage key={m} api={api} token={token} mediaId={m} rendition="thumb" width={280} height={190} />
            ))}
          </ScrollView>
          <View style={{ flex: 1, gap: 12 }}>
            <Text testID="day-summary" style={{ color: theme.muted, fontSize: theme.small }}>
              {`${current.date} · ${current.media.length} photos${current.route.inferred ? " · route inferred" : ""}`}
            </Text>
            {trip.narrative ? (
              <Text testID="narrative" style={{ color: theme.fg, fontSize: theme.small, lineHeight: 30 }}>{trip.narrative.text}</Text>
            ) : (
              <Text style={{ color: theme.muted, fontSize: theme.small }}>The story of this trip is being written.</Text>
            )}
          </View>
        </View>
      )}
    </View>
  );
}
