import { useState } from "react";
import { Pressable, Text, View } from "react-native";
import { theme } from "../theme";

export interface RailItem {
  key: string;
  label: string;
  onPress: () => void;
  testID?: string;
  badge?: number;
  preferred?: boolean;
}

/**
 * The left navigation rail on every screen but the show. Items are the household's
 * places: Memories, Map, People, Ask, Add photos, Review. The active one is lit; a
 * badge carries pending review count.
 */
export function Rail({ items, active, tvName }: { items: RailItem[]; active: string; tvName?: string }) {
  return (
    <View
      testID="rail"
      style={{
        position: "absolute",
        left: 0,
        top: 0,
        bottom: 0,
        width: theme.rail,
        paddingTop: theme.safe.y,
        paddingBottom: theme.safe.y,
        paddingLeft: theme.actionSafe.x,
        paddingRight: 24,
        gap: 6,
        backgroundColor: "#0b0b0fee",
      }}
    >
      <View style={{ flexDirection: "row", alignItems: "center", gap: 12, height: 60, marginBottom: 28 }}>
        <View style={{ width: 44, height: 44, borderRadius: 10, backgroundColor: theme.panel, justifyContent: "center", alignItems: "center" }}>
          <View style={{ width: 22, height: 22, borderRadius: 11, borderWidth: 4, borderColor: theme.accent }} />
        </View>
        <Text style={{ color: theme.fg, fontSize: 28, fontWeight: "600" }}>Mantelpiece</Text>
      </View>
      {items.map((item) => (
        <RailButton key={item.key} item={item} active={item.key === active} />
      ))}
      {tvName ? <Text style={{ marginTop: "auto", color: theme.dim, fontSize: theme.credit, paddingHorizontal: 18 }}>{tvName}</Text> : null}
    </View>
  );
}

function RailButton({ item, active }: { item: RailItem; active: boolean }) {
  const [focused, setFocused] = useState(false);
  const lit = active || focused;
  return (
    <Pressable
      testID={item.testID}
      accessibilityRole="button"
      hasTVPreferredFocus={item.preferred}
      onFocus={() => setFocused(true)}
      onBlur={() => setFocused(false)}
      onPress={item.onPress}
      style={{
        flexDirection: "row",
        alignItems: "center",
        gap: 16,
        height: 60,
        paddingHorizontal: 18,
        borderRadius: 14,
        backgroundColor: focused ? theme.accent : active ? "#ffffff12" : "transparent",
      }}
    >
      <View style={{ width: 10, height: 10, borderRadius: 5, backgroundColor: focused ? theme.onAccent : active ? theme.accent : theme.dim }} />
      <Text style={{ color: focused ? theme.onAccent : lit ? theme.fg : theme.muted, fontSize: theme.small, fontWeight: lit ? "600" : "500" }}>{item.label}</Text>
      {item.badge ? (
        <View style={{ marginLeft: "auto", minWidth: 28, height: 28, borderRadius: 14, backgroundColor: focused ? theme.onAccent : theme.accent, paddingHorizontal: 8, justifyContent: "center", alignItems: "center" }}>
          <Text style={{ color: focused ? theme.accent : theme.onAccent, fontSize: theme.credit, fontWeight: "700" }}>{item.badge}</Text>
        </View>
      ) : null}
    </Pressable>
  );
}
