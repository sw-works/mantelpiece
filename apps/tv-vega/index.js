import { createElement, useEffect, useState } from "react";
import { AppRegistry } from "react-native";
import { App } from "@memorylane/tv-ui";
import { apiBase, name } from "./app.json";
import { audioPlayer } from "./src/audio";
import { deviceName } from "./src/deviceName";
import { sessionStore } from "./src/sessionStore";

// The component name must equal the main interactive component id in manifest.toml.
// The shell injects what is platform-specific: where the session lives, the TV's name and
// the API base (app.json `apiBase`, written by scripts/tv-config.sh from the stack outputs).
// Narration (F-709) plays through the system media port when the platform has one and
// falls back to captions when it does not; the flag itself lives on the household.
const store = sessionStore();
const audio = audioPlayer();

// The TV's own name is the household name offered on first launch; wait for it (it
// answers or times out within a second or two) so the name step never flickers.
function Root() {
  const [tvName, setTvName] = useState(null);
  useEffect(() => {
    let alive = true;
    deviceName().then((n) => alive && setTvName(n));
    return () => {
      alive = false;
    };
  }, []);
  if (tvName === null) return null;
  return createElement(App, { apiBase: apiBase || undefined, sessionStore: store, tvName, audio, narration: true });
}

AppRegistry.registerComponent(name, () => Root);
