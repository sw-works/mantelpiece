// link.<domain>/?c=<code>: approve the TV's device code with the owner's Amazon account,
// then continue to the uploader as the owner.

import { Api } from "../api.js";
import { beginSignIn, completeSignIn, savedTokens } from "../auth.js";
import { loadConfig } from "../config.js";
import { el, mount } from "../ui.js";

const OWNER_TOKEN = "ml.owner.token";

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
    storage.setItem(OWNER_TOKEN, approved.token);
    const uploadUrl = `${location.origin.replace("link.", "upload.")}/?owner=1`;
    mount(
      root,
      el("h1", {}, "Your TV is signed in"),
      el("p", {}, "Now pick the photos you want on the big screen."),
      el("a", { class: "button", href: uploadUrl }, "Choose photos"),
    );
  } catch (e) {
    mount(root, el("h1", {}, "That code did not work"), el("p", {}, e instanceof Error ? e.message : String(e)));
  }
}
