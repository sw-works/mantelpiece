import { useEffect, useState } from "react";
import { Text, View } from "react-native";
import { type CodeView, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { QrView } from "../components/QrView";
import { theme } from "../theme";

/** F-151, F-409: full-screen QR for the household or one trip, with the live ticker. */
export function Invite({
  api,
  token,
  tripId,
  ticker,
  onBack,
}: {
  api: TvApi;
  token: string;
  tripId: string | null;
  ticker: string | null;
  onBack: () => void;
}) {
  const [code, setCode] = useState<CodeView | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    (tripId ? api.tripCode(token, tripId) : api.householdCode(token))
      .then((c) => alive && setCode(c))
      .catch((e: unknown) => alive && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      alive = false;
    };
  }, [api, token, tripId]);

  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, padding: 64, flexDirection: "row", gap: 64, alignItems: "center" }}>
      <View style={{ flex: 1, gap: theme.gap }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>
          {tripId ? "Everyone on the trip: scan this" : "Add photos from your phone"}
        </Text>
        <Text style={{ color: theme.muted, fontSize: theme.body }}>
          Photos land in this {tripId ? "trip" : "household"}, credited to whoever sends them. No app to install.
        </Text>
        {code && (
          <Text testID="code" style={{ color: theme.accent, fontSize: 72, fontWeight: "800", letterSpacing: 6 }}>{code.code}</Text>
        )}
        {ticker && <Text testID="ticker" style={{ color: theme.accent, fontSize: theme.body }}>{ticker}</Text>}
        {error && <Text testID="error" style={{ color: "#ff7a7a", fontSize: theme.body }}>{error}</Text>}
        <FocusButton label="Done" onPress={onBack} preferred testID="done" />
      </View>
      {code && <QrView text={code.link} size={480} />}
    </View>
  );
}
