import { View } from "react-native";
import { type TvApi } from "../api/client";
import { RemoteImage } from "./RemoteImage";
import { theme } from "../theme";

/**
 * A photo across the top of the screen, dimmed so text over it reads; a solid band
 * fades it into the ground below. No gradient primitive on React Native core, so
 * the fade is two flat layers.
 */
export function Backdrop({ api, token, mediaId, height = 700, dim = 0.55 }: { api: TvApi; token: string; mediaId: string | null | undefined; height?: number; dim?: number }) {
  if (!mediaId) return null;
  return (
    <View pointerEvents="none" style={{ position: "absolute", left: 0, top: 0, right: 0, height }}>
      <RemoteImage api={api} token={token} mediaId={mediaId} rendition="tv" width={1920} height={height} radius={0} />
      <View style={{ position: "absolute", left: 0, top: 0, right: 0, bottom: 0, backgroundColor: theme.bg, opacity: dim }} />
      <View style={{ position: "absolute", left: 0, right: 0, bottom: 0, height: height * 0.35, backgroundColor: theme.bg, opacity: 0.85 }} />
    </View>
  );
}
