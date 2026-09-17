// link.<domain>/?c=<code>: approve the TV's device code with the owner's Amazon account,
// then continue to the uploader as the owner.

import { Api } from "../api.js";
import { OWNER_TOKEN_KEY, beginSignIn, completeSignIn, ownerHandoffUrl, savedTokens } from "../auth.js";
import { loadConfig } from "../config.js";
import { el, eyebrow, icon, mount, topbar } from "../ui.js";

/** The name or email inside a Cognito id token, for "Signed in as …"; null when unreadable. */
function signedInAs(idToken: string): string | null {
  try {
    const payload = idToken.split(".")[1] ?? "";
    const json = JSON.parse(atob(payload.replace(/-/g, "+").replace(/_/g, "/"))) as Record<string, unknown>;
    const name = json["name"] ?? json["email"];
    return typeof name === "string" && name.length > 0 ? name : null;
  } catch {
    return null;
  }
}

function page(title: string, ...rest: (Node | string)[]): (Node | string)[] {
  return [topbar(), el("div", { class: "stack" }, el("h1", {}, title), ...rest)];
}

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

  // ?join=<trip code>: scanned another household's Invite while this phone is linked to a
  // TV, so that TV joins the trip (F-411, US7).
  const join = url.searchParams.get("join");
  if (join) {
    const owner = storage.getItem(OWNER_TOKEN_KEY);
    if (!owner) {
      mount(root, ...page("Link your TV first", el("p", { class: "lead" }, "Scan the sign-in code on your own TV, then scan this invite again.")));
      return;
    }
    mount(root, ...page(`Joining ${join}…`));
    try {
      const r = await api.joinTrip(owner, join);
      mount(root, ...page(r.already ? "Already joined" : "Your TV joined the trip", el("p", { class: "lead" }, `${r.name ?? "The trip"} now shows on your Fire TV, live.`)));
    } catch (e) {
      mount(root, ...page("That code did not open a trip", el("p", { class: "error" }, e instanceof Error ? e.message : String(e))));
    }
    return;
  }

  const code = url.searchParams.get("c");
  if (!code) {
    mount(root, ...page("MemoryLane", el("p", { class: "lead" }, "Open this page by scanning the code on your TV.")));
    return;
  }

  const tokens = savedTokens(storage);
  if (!tokens) {
    mount(root, ...page("Signing in with Amazon…"));
    location.assign(await beginSignIn(auth, storage, location.href));
    return;
  }

  mount(root, topbar(), el("div", { class: "stack" }, eyebrow(`Code ${code}`), el("h1", {}, `Approving ${code}…`)));
  try {
    const approved = await api.approveDeviceCode(code, tokens.idToken);
    storage.setItem(OWNER_TOKEN_KEY, approved.token);
    // Straight on to the uploader as the owner: no second sign-in (US1).
    const uploadUrl = ownerHandoffUrl(config.uploadBase ?? location.origin.replace("link.", "upload."), approved.token);
    const who = signedInAs(tokens.idToken);
    mount(
      root,
      topbar(),
      el(
        "div",
        { class: "stack" },
        eyebrow(`Code ${code}`),
        el("h1", {}, "Your TV is signed in"),
        el("p", { class: "lead" }, "Your Fire TV now belongs to your household. Photos you add from this phone land there."),
      ),
      el(
        "div",
        { class: "card" },
        el("div", { class: "iconbox" }, icon("tv", 26)),
        el("div", { class: "titles" }, el("span", { class: "title" }, "Your Fire TV"), el("p", { class: "small" }, who ? `Signed in as ${who} · just now` : "Signed in with Amazon · just now")),
        el("span", { class: "ok" }, icon("check", 22)),
      ),
      el("a", { class: "button", href: uploadUrl }, icon("images", 20), el("span", {}, "Choose photos")),
      el("p", { class: "small" }, "Taking you to your photos in a moment. Next time, this phone opens straight to them."),
      el(
        "div",
        { class: "footer" },
        eyebrow("Invite others later"),
        el("p", { class: "meta" }, "Press Add photos on the TV. Anyone who scans that code adds to your trip, no account needed."),
      ),
    );
    location.replace(uploadUrl);
  } catch (e) {
    mount(root, ...page("That code did not work", el("p", { class: "error" }, e instanceof Error ? e.message : String(e))));
  }
}
