import { useState } from "react";
import { Text, TextInput, View } from "react-native";
import { type Trip, type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { RemoteImage } from "../components/RemoteImage";
import { theme } from "../theme";

const CODE_CHARS = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".split("");

/**
 * F-411, F-162: join a trip from another Fire TV by the same code the phones use. The
 * code can be typed with the on-screen keyboard or picked from the grid with the D-pad;
 * a preview shows what will be joined before anything happens.
 */
export function JoinTrip({ api, token, onJoined, onBack }: { api: TvApi; token: string; onJoined: (tripId: string) => void; onBack: () => void }) {
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

  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: theme.gap }}>
      <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center" }}>
        <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>Join a trip</Text>
        <FocusButton label="Back" onPress={onBack} testID="back" />
      </View>
      <Text style={{ color: theme.muted, fontSize: theme.body }}>Enter the six-character code shown on the other TV's Invite screen. You will see their trip here, live, and can pin your own favourites.</Text>
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
          placeholder="ABC234"
          placeholderTextColor={theme.muted}
          style={{ color: theme.accent, fontSize: 56, fontWeight: "800", letterSpacing: 8, backgroundColor: theme.panel, padding: 12, borderRadius: theme.radius, width: 360 }}
        />
        <FocusButton label="Clear" onPress={() => { setCode(""); setPreview(null); }} testID="clear" />
      </View>
      <View style={{ flexDirection: "row", flexWrap: "wrap", gap: 8 }}>
        {CODE_CHARS.map((c) => (
          <FocusButton key={c} label={c} onPress={() => type(c)} testID={`key-${c}`} />
        ))}
      </View>
      {error && <Text testID="error" style={{ color: "#ff7a7a", fontSize: theme.body }}>{error}</Text>}
      {preview && (
        <View testID="preview" style={{ flexDirection: "row", gap: theme.gap, alignItems: "center", backgroundColor: theme.panel, padding: 24, borderRadius: theme.radius }}>
          {preview.trip.hero && <RemoteImage api={api} token={token} mediaId={preview.trip.hero} rendition="thumb" width={240} height={150} />}
          <View style={{ flex: 1, gap: 8 }}>
            <Text testID="preview-name" style={{ color: theme.fg, fontSize: theme.body, fontWeight: "600" }}>{preview.name}</Text>
            <Text style={{ color: theme.muted, fontSize: theme.small }}>
              {`${preview.trip.days.length} days · ${preview.trip.highlights.length} highlights${preview.households ? ` · shared by ${preview.households}` : ""}`}
            </Text>
          </View>
          <FocusButton label={busy ? "Joining…" : "Join"} onPress={() => void join()} preferred testID="join" />
        </View>
      )}
    </View>
  );
}
