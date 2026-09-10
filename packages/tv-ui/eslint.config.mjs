import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["node_modules/**", "babel.config.js", "jest.config.js", "eslint.config.mjs"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
);
