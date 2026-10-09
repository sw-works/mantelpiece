import { useEffect, useState } from "react";
import { Image, View } from "react-native";
import { type TvApi } from "../api/client";
import { theme } from "../theme";

/** Resolves a presigned rendition URL then shows it. Placeholder panel while loading. */
export function RemoteImage({
  api,
  token,
  mediaId,
  rendition = "tv",
  width,
  height,
  radius = theme.radius,
  resizeMode = "cover",
  onSize,
}: {
  api: TvApi;
  token: string;
  mediaId: string;
  rendition?: "thumb" | "tv" | "fieldnote" | "fieldnote_map";
  width: number;
  height: number;
  radius?: number;
  resizeMode?: "cover" | "contain";
  /** The source's own size once loaded, for laying things over a `contain` image. */
  onSize?: (width: number, height: number) => void;
}) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    api
      .mediaUrl(token, mediaId, rendition)
      .then((r) => alive && setUrl(r.url))
      .catch(() => alive && setUrl(null));
    return () => {
      alive = false;
    };
  }, [api, token, mediaId, rendition]);
  if (!url) return <View style={{ width, height, backgroundColor: theme.panel, borderRadius: radius }} />;
  return (
    <Image
      testID={`image-${mediaId}-${rendition}`}
      source={{ uri: url }}
      style={{ width, height, borderRadius: radius }}
      resizeMode={resizeMode}
      onLoad={(e) => {
        const s = e.nativeEvent?.source;
        if (s && onSize) onSize(s.width, s.height);
      }}
    />
  );
}
