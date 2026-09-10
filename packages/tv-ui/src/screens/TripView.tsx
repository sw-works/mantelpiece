import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { clockHints, creditFor, type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
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

/** F-403: day timeline, route map, narrative, highlights, Invite (F-409). */
export function TripView({
  api,
  token,
  tripId,
  onBack,
  onInvite,
  onHighlights,
}: {
  api: TvApi;
  token: string;
  tripId: string;
  onBack: () => void;
  onInvite: (tripId: string) => void;
  onHighlights?: (tripId: string) => void;
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
  const credits = current ? dayCredits(trip, current.media) : [];
  const hints = clockHints(trip);
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: theme.gap }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>{trip.name}</Text>
        <View style={{ flexDirection: "row", gap: 16 }}>
          {onHighlights && <FocusButton label="Highlights" onPress={() => onHighlights(trip.id)} testID="highlights" />}
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
              <View key={m} style={{ gap: 6 }}>
                <RemoteImage api={api} token={token} mediaId={m} rendition="thumb" width={280} height={190} />
                <Text testID={`credit-${m}`} style={{ color: theme.muted, fontSize: theme.small }}>
                  {creditFor(trip, m) ?? ""}
                </Text>
              </View>
            ))}
          </ScrollView>
          <View style={{ flex: 1, gap: 12 }}>
            <Text testID="day-summary" style={{ color: theme.muted, fontSize: theme.small }}>
              {`${current.date} · ${current.media.length} photos${credits.length ? ` from ${credits.join(", ")}` : ""}${current.route.inferred ? " · route inferred" : ""}`}
            </Text>
            {hints.length > 0 && (
              <Text testID="clock-hints" style={{ color: theme.muted, fontSize: theme.small }}>
                {hints.join(" · ")}
              </Text>
            )}
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
