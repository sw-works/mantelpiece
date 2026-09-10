import { useState } from "react";
import { Text, View } from "react-native";
import { type TvApi } from "../api/client";
import { FocusButton } from "../components/FocusButton";
import { theme } from "../theme";

export interface Proposal {
  title: string;
  before: number;
  after: number;
  /** The action body posted to /confirmations, e.g. { action: "apply_dedup", group_ids: [...] }. */
  action: Record<string, unknown>;
  sample: string[];
}

/** F-410: nothing irreversible happens until the user confirms here on the remote. */
export function ReviewChanges({
  api,
  token,
  proposal,
  onConfirmed,
  onCancel,
}: {
  api: TvApi;
  token: string;
  proposal: Proposal;
  onConfirmed: (confirmation: string) => void;
  onCancel: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const confirm = async () => {
    setBusy(true);
    try {
      const minted = await api.mintConfirmation(token, proposal.action);
      onConfirmed(minted.token);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg, paddingHorizontal: theme.safe.x, paddingVertical: theme.safe.y, gap: theme.gap, justifyContent: "center" }}>
      <Text style={{ color: theme.fg, fontSize: theme.title, fontWeight: "700" }}>Review changes</Text>
      <Text style={{ color: theme.fg, fontSize: theme.body }}>{proposal.title}</Text>
      <Text testID="counts" style={{ color: theme.muted, fontSize: theme.body }}>
        {`${proposal.before} before · ${proposal.after} after`}
      </Text>
      {error && <Text testID="error" style={{ color: "#ff7a7a", fontSize: theme.body }}>{error}</Text>}
      <View style={{ flexDirection: "row", gap: 16 }}>
        <FocusButton label="Keep everything" onPress={onCancel} preferred testID="cancel" />
        <FocusButton label={busy ? "Confirming…" : "Apply"} onPress={() => void confirm()} testID="apply" />
      </View>
    </View>
  );
}
