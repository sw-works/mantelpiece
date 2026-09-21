import { useState } from "react";
import { Text, TextInput, View } from "react-native";
import { type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { Eyebrow, Meta, Title } from "../components/Type";
import { theme } from "../theme";

/**
 * First launch. The household is named here, on the TV, and nowhere else. The name starts
 * as the TV's own, so Select on Continue is the whole step; Change name opens the keyboard.
 */
export function Welcome({
  api,
  deviceId,
  defaultName,
  onReady,
}: {
  api: TvApi;
  deviceId: string;
  defaultName: string;
  onReady: (session: { token: string; household: string; name: string }) => void;
}) {
  const [name, setName] = useState(defaultName);
  const [editing, setEditing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const shown = name.trim() || defaultName;

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      const made = await api.setupTv(deviceId, shown);
      onReady({ token: made.token, household: made.household, name: made.household_name });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };

  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y }}>
      <Text style={{ color: theme.fg, fontSize: 30, fontWeight: "600" }}>MemoryLane</Text>
      <View style={{ flex: 1, justifyContent: "center", gap: 28, maxWidth: 1300 }}>
        <Eyebrow>First time on this TV</Eyebrow>
        <Title size={84}>Your trips, on the big screen.</Title>
        <Meta size={theme.body}>Phones that connect see this name. It starts as the name of this TV.</Meta>
        {editing ? (
          <TextInput
            testID="name-input"
            value={name}
            onChangeText={setName}
            onSubmitEditing={() => setEditing(false)}
            onBlur={() => setEditing(false)}
            autoFocus
            maxLength={40}
            returnKeyType="done"
            selectionColor={theme.accent}
            style={{ color: theme.fg, fontSize: 72, fontWeight: "700", paddingVertical: 18, paddingHorizontal: 28, borderRadius: 24, borderWidth: 4, borderColor: theme.accent, backgroundColor: theme.panel }}
          />
        ) : (
          <Text testID="household-name" numberOfLines={1} style={{ color: theme.fg, fontSize: 72, fontWeight: "700", paddingVertical: 18, paddingHorizontal: 28, borderRadius: 24, borderWidth: 4, borderColor: theme.panel2, backgroundColor: theme.panel }}>
            {shown}
          </Text>
        )}
        {error && <Text testID="error" style={{ color: theme.bad, fontSize: theme.body }}>{error}</Text>}
        <View style={{ flexDirection: "row", gap: 20 }}>
          <FocusButton testID="continue" label={busy ? "Setting up…" : "Continue"} onPress={start} disabled={busy} preferred={!editing} />
          <FocusButton testID="change-name" label="Change name" variant="ghost" onPress={() => setEditing(true)} disabled={busy} />
        </View>
      </View>
    </View>
  );
}
