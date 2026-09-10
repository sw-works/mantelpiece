// Hardware remote keys on Fire OS (Android TV). Back arrives through BackHandler; the
// media keys are delivered as key events the app does not need yet. The Vega OS variant
// lives in useRemoteKeys.kepler.ts and is picked by Metro's sourceExts.

import { useEffect } from "react";
import { BackHandler } from "react-native";

export interface RemoteKeyHandlers {
  /** Return nothing to consume Back. Leave undefined to let the platform exit the app. */
  onBack?: () => void;
  onPlayPause?: () => void;
}

export function useRemoteKeys({ onBack }: RemoteKeyHandlers) {
  useEffect(() => {
    const sub = BackHandler.addEventListener("hardwareBackPress", () => {
      if (!onBack) return false;
      onBack();
      return true;
    });
    return () => sub.remove();
  }, [onBack]);
}
