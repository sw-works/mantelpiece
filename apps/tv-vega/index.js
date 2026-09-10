import { createElement } from "react";
import { AppRegistry } from "react-native";
import { App } from "@memorylane/tv-ui";
import { name } from "./app.json";
import { audioPlayer } from "./src/audio";
import { sessionStore } from "./src/sessionStore";

// The component name must equal the main interactive component id in manifest.toml.
// The shell injects what is platform-specific: where the session lives and the TV's name.
// Narration (F-709) plays through the system media port when the platform has one and
// falls back to captions when it does not; the flag itself lives on the household.
const store = sessionStore();
const audio = audioPlayer();
AppRegistry.registerComponent(name, () => () => createElement(App, { sessionStore: store, tvName: "Fire TV", audio, narration: true }));
