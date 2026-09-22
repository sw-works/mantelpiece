import { useEffect, useState } from "react";
import { Text, View } from "react-native";
import { type CodeView, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { QrView } from "../components/QrView";
import { Eyebrow, Meta, Title } from "../components/Type";
import { theme } from "../theme";

/** F-151, F-409: full-screen QR for the household or one trip, with the live line. */
export function Invite({
  api,
  token,
  tripId,
  tripName,
  ticker,
  onBack,
}: {
  api: TvApi;
  token: string;
  tripId: string | null;
  /** Shown over the title when inviting to one trip. */
  tripName?: string;
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
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: 40 }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: 30, fontWeight: "600" }}>Mantelpiece</Text>
        <FocusButton label="Done" variant="ghost" onPress={onBack} preferred testID="done" />
      </View>
      <View style={{ flex: 1, flexDirection: "row", gap: 64 }}>
        <View style={{ flex: 1, gap: 24 }}>
          <Eyebrow>{tripId ? `${tripName ?? "This trip"} · invite` : "Your household · add photos"}</Eyebrow>
          <Title size={84}>{tripId ? "Everyone on the trip: scan this." : "Add photos from your phone."}</Title>
          <Meta size={theme.body}>{`Photos land in this ${tripId ? "trip" : "household"}, credited to whoever sends them. No app to install.`}</Meta>
          {code && (
            <>
              <Text testID="code" style={{ color: theme.accent, fontSize: 112, fontWeight: "700", letterSpacing: 16, lineHeight: 124 }}>
                {code.code}
              </Text>
              <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
                <FocusButton label="New code" variant="ghost" testID="rotate" onPress={() => api.rotateCode(token, code.code).then(setCode).catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))} />
                <Meta testID="counts">{`${code.contributors ?? 0} contributors · ${code.member_households ?? 0} households watching`}</Meta>
              </View>
            </>
          )}
          {ticker && (
            <View style={{ flexDirection: "row", alignItems: "center", gap: 14, padding: 16, borderRadius: 14, backgroundColor: "#ffb45418", alignSelf: "flex-start", marginTop: 18 }}>
              <View style={{ width: 10, height: 10, borderRadius: 5, backgroundColor: theme.accent }} />
              <Text testID="ticker" style={{ color: theme.fg, fontSize: theme.small }}>
                {ticker}
              </Text>
            </View>
          )}
          {error && <Text testID="error" style={{ color: theme.bad, fontSize: theme.body }}>{error}</Text>}
        </View>
        {code && (
          <View style={{ alignItems: "center", gap: 22 }}>
            <View style={{ padding: 28, borderRadius: 28, backgroundColor: "#fff" }}>
              <QrView text={code.link} size={480} />
            </View>
            <Meta>Works on another Fire TV too: Join a trip, then the code</Meta>
          </View>
        )}
      </View>
    </View>
  );
}
