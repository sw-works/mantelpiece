import type { ReactNode } from "react";
import { View } from "react-native";
import { Rail, type RailItem } from "./Rail";
import { Title } from "./Type";
import { CONTENT_X, theme } from "../theme";

/** Navigation every screen can offer through the rail; App builds one per session. */
export interface Nav {
  items: RailItem[];
  tvName?: string;
}

/**
 * Rail on the left, title and actions across the top, content below. Screens that own
 * the whole picture (slideshow, invite, onboarding) do not use it.
 */
export function Screen({ nav, active, title, actions, children, contentStyle, backdrop }: { nav?: Nav; active: string; title?: string; actions?: ReactNode; children?: ReactNode; contentStyle?: object; /** Full-bleed picture behind everything (the home hero). */ backdrop?: ReactNode }) {
  const left = nav ? CONTENT_X : theme.safe.x;
  return (
    <View style={{ flex: 1, backgroundColor: theme.bg }}>
      {backdrop}
      {nav && <Rail items={nav.items} active={active} tvName={nav.tvName} />}
      <View style={[{ flex: 1, marginLeft: left, marginRight: theme.safe.x, paddingTop: theme.safe.y, paddingBottom: theme.safe.y, gap: theme.gap }, contentStyle]}>
        {(title || actions) && (
          <View style={{ flexDirection: "row", justifyContent: "space-between", alignItems: "center", gap: 24 }}>
            {title ? <Title>{title}</Title> : <View />}
            {actions ? <View style={{ flexDirection: "row", gap: 14, alignItems: "center" }}>{actions}</View> : null}
          </View>
        )}
        {children}
      </View>
    </View>
  );
}
