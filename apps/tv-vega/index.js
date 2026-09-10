import { createElement } from "react";
import { AppRegistry } from "react-native";
import { App } from "@memorylane/tv-ui";
import { name } from "./app.json";
import { sessionStore } from "./src/sessionStore";

// The component name must equal the main interactive component id in manifest.toml.
// The shell injects what is platform-specific: where the session lives and the TV's name.
const store = sessionStore();
AppRegistry.registerComponent(name, () => () => createElement(App, { sessionStore: store, tvName: "Fire TV" }));
