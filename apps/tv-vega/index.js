import { AppRegistry } from "react-native";
import { App } from "@memorylane/tv-ui";
import { name } from "./app.json";

// The component name must equal the main interactive component id in manifest.toml.
AppRegistry.registerComponent(name, () => App);
