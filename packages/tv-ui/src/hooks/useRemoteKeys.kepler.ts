// Vega OS variant: the Vega SDK's React Native ships TVEventHandler, which reports remote
// presses as HWEvents with eventType "back", "select", "playpause", "left" and so on.
// Only key-down (eventKeyAction 0, or undefined on older runtimes) is acted on.
// TVEventHandler is looked up at runtime because upstream React Native 0.87 has no such
// export and this file must still typecheck against it.

import { useEffect } from "react";
import * as ReactNative from "react-native";
import type { RemoteKeyHandlers } from "./useRemoteKeys";

interface HWEvent {
  eventType: string;
  eventKeyAction?: number;
}

interface TVEventHandlerLike {
  enable(component: unknown, callback: (component: unknown, event: HWEvent) => void): void;
  disable(): void;
}

export type { RemoteKeyHandlers };

export function useRemoteKeys({ onBack, onPlayPause }: RemoteKeyHandlers) {
  useEffect(() => {
    const Ctor = (ReactNative as unknown as { TVEventHandler?: new () => TVEventHandlerLike }).TVEventHandler;
    if (!Ctor) return;
    const handler = new Ctor();
    handler.enable(null, (_component, event) => {
      if (event.eventKeyAction !== undefined && event.eventKeyAction !== 0) return;
      if (event.eventType === "back") onBack?.();
      if (event.eventType === "playpause") onPlayPause?.();
    });
    return () => handler.disable();
  }, [onBack, onPlayPause]);
}
