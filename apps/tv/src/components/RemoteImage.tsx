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
}: {
  api: TvApi;
  token: string;
  mediaId: string;
  rendition?: "thumb" | "tv";
  width: number;
  height: number;
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
  if (!url) return <View style={{ width, height, backgroundColor: theme.panel, borderRadius: theme.radius }} />;
  return <Image source={{ uri: url }} style={{ width, height, borderRadius: theme.radius }} resizeMode="cover" />;
}
