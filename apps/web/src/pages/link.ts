// link.<domain>/?c=<code>: approve the TV's device code with the owner's Amazon account,
// then continue to the uploader as the owner.

import { Api } from "../api.js";
import { OWNER_TOKEN_KEY, beginSignIn, completeSignIn, ownerHandoffUrl, savedTokens } from "../auth.js";
import { loadConfig } from "../config.js";
import { el, mount } from "../ui.js";

export async function main(root: HTMLElement, location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const auth = { domain: config.cognitoDomain, clientId: config.cognitoClientId, redirectUri: `${location.origin}/callback` };
  const url = new URL(location.href);

  if (url.pathname === "/callback") {
    const { returnTo } = await completeSignIn(auth, storage, location.href);
    location.replace(returnTo);
    return;
  }

  const code = url.searchParams.get("c");
  if (!code) {
    mount(root, el("h1", {}, "MemoryLane"), el("p", {}, "Open this page by scanning the code on your TV."));
    return;
  }

  const tokens = savedTokens(storage);
  if (!tokens) {
    mount(root, el("h1", {}, "Signing in with Amazon…"));
    location.assign(await beginSignIn(auth, storage, location.href));
    return;
  }

  mount(root, el("h1", {}, `Approving ${code}…`));
  try {
    const approved = await api.approveDeviceCode(code, tokens.idToken);
    storage.setItem(OWNER_TOKEN_KEY, approved.token);
    // Straight on to the uploader as the owner: no second sign-in (US1).
    const uploadUrl = ownerHandoffUrl(location.origin.replace("link.", "upload."), approved.token);
    mount(
      root,
      el("h1", {}, "Your TV is signed in"),
      el("p", {}, "Taking you to your photos…"),
      el("a", { class: "button", href: uploadUrl }, "Choose photos"),
    );
    location.replace(uploadUrl);
  } catch (e) {
    mount(root, el("h1", {}, "That code did not work"), el("p", {}, e instanceof Error ? e.message : String(e)));
  }
}
