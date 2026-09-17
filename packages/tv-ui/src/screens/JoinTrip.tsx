import { useState } from "react";
import { Text, TextInput, View } from "react-native";
import { type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { type Nav, Screen } from "../components/Screen";
import { Eyebrow, Meta } from "../components/Type";
import { theme } from "../theme";

const CODE_CHARS = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".split("");

/**
 * F-411, F-162: join a trip from another Fire TV by the same code the phones use. The
 * code can be typed with the on-screen keyboard or picked from the grid with the D-pad;
 * a preview shows what will be joined before anything happens.
 */
export function JoinTrip({ api, token, onJoined, onBack, nav }: { api: TvApi; token: string; onJoined: (tripId: string) => void; onBack: () => void; nav?: Nav }) {
  const [code, setCode] = useState("");
  const [preview, setPreview] = useState<{ name: string; households: string | null; trip: Trip } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const lookup = async (value: string) => {
    if (value.length < 6) return;
    setBusy(true);
    setError(null);
    try {
      const v = await api.previewTrip(value);
      setPreview({ name: v.trip.name, households: v.household_name, trip: v.trip });
    } catch (e) {
      setPreview(null);
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  const type = (c: string) => {
    const next = (code + c).slice(0, 6);
    setCode(next);
    if (next.length === 6) void lookup(next);
  };
  const join = async () => {
    setBusy(true);
    setError(null);
    try {
      const r = await api.joinTrip(token, code);
      onJoined(r.trip);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };
  const cells = Array.from({ length: 6 }, (_, i) => code[i] ?? "");

  return (
    <Screen nav={nav} active="join" title="Join a trip" actions={<FocusButton label="Back" variant="ghost" onPress={onBack} testID="back" />}>
      <View style={{ flexDirection: "row", gap: 60, flex: 1 }}>
        <View style={{ width: 840, gap: 26 }}>
          <Meta size={theme.body}>Enter the six-character code from the other TV's Invite screen. Their trip shows here, live, and you pick your own keepers.</Meta>
          <View style={{ flexDirection: "row", gap: 14, alignItems: "center" }}>
            {cells.map((c, i) => (
              <View key={i} style={{ width: 96, height: 120, borderRadius: 18, backgroundColor: c ? theme.panel2 : theme.panel, justifyContent: "center", alignItems: "center", borderWidth: 4, borderColor: !c && i === code.length ? theme.accent : "transparent" }}>
                <Text style={{ color: theme.accent, fontSize: 72, fontWeight: "700" }}>{c}</Text>
              </View>
            ))}
          </View>
          <View style={{ flexDirection: "row", gap: 16, alignItems: "center" }}>
            <TextInput
              testID="code"
              value={code}
              onChangeText={(v) => {
                const clean = v.toUpperCase().replace(/[^A-Z0-9]/g, "").slice(0, 6);
                setCode(clean);
                if (clean.length === 6) void lookup(clean);
              }}
              autoCapitalize="characters"
              maxLength={6}
              placeholder="Or type the code"
              placeholderTextColor={theme.dim}
              style={{ color: theme.fg, fontSize: theme.body, backgroundColor: theme.panel, paddingHorizontal: 18, height: 64, borderRadius: 32, width: 360 }}
            />
            <FocusButton
              label="Clear"
              variant="ghost"
              onPress={() => {
                setCode("");
                setPreview(null);
              }}
              testID="clear"
            />
          </View>
          <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 12, width: 620 }}>
            {CODE_CHARS.map((c) => (
              <FocusButton key={c} variant="chip" label={c} onPress={() => type(c)} testID={`key-${c}`} />
            ))}
          </View>
          {error && <Text testID="error" style={{ color: theme.bad, fontSize: theme.body }}>{error}</Text>}
        </View>
        <View style={{ flex: 1, gap: 18 }}>
          {preview && (
            <View testID="preview" style={{ gap: 18 }}>
              <Eyebrow>{preview.households ? `Preview · shared by ${preview.households}` : "Preview"}</Eyebrow>
              <View style={{ borderRadius: 22, backgroundColor: theme.panel, overflow: "hidden" }}>
                {preview.trip.hero && <RemoteImage api={api} token={token} mediaId={preview.trip.hero} rendition="thumb" width={560} height={300} radius={0} />}
                <View style={{ padding: 22, gap: 8 }}>
                  <Text testID="preview-name" style={{ color: theme.fg, fontSize: theme.heading, fontWeight: "600" }}>{preview.name}</Text>
                  <Meta>{`${preview.trip.days.length} days · ${preview.trip.highlights.length} highlights`}</Meta>
                </View>
              </View>
              <FocusButton label={busy ? "Joining…" : "Join this trip"} onPress={() => void join()} preferred testID="join" />
            </View>
          )}
        </View>
      </View>
    </Screen>
  );
}
