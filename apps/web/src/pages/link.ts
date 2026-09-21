// link.<domain>/?p=<code>: pair this phone with the TV. Give a name, wait for someone at
// the TV to approve, then continue to the uploader. No account.
// link.<domain>/?c=<code>: the older sign-in, approving a device code with an Amazon account.

import { Api } from "../api.js";
import { NAME_KEY, clearWaiting, readWaiting, saveWaiting, untilDecided, type Waiting } from "../pair.js";
import { OWNER_TOKEN_KEY, beginSignIn, completeSignIn, ownerHandoffUrl, savedTokens } from "../auth.js";
import { loadConfig } from "../config.js";
import { button, el, eyebrow, field, icon, mount, topbar } from "../ui.js";

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

/** The three pairing screens: give a name, wait for the TV, go add photos. */
async function pair(root: HTMLElement, api: Api, code: string, uploadBase: string, location: Location, storage: Storage): Promise<void> {
  const tvCard = (tvName: string, state: Node, line: string) =>
    el("div", { class: "card" }, el("div", { class: "iconbox" }, icon("tv", 26)), el("div", { class: "titles" }, el("span", { class: "title" }, tvName), el("p", { class: "small" }, line)), state);

  const connected = (tvName: string, token: string) => {
    storage.setItem(OWNER_TOKEN_KEY, token);
    clearWaiting(storage);
    const uploadUrl = ownerHandoffUrl(uploadBase, token);
    mount(
      root,
      topbar(),
      el("div", { class: "stack" }, eyebrow("Connected"), el("h1", {}, "You're in"), el("p", { class: "lead" }, `Photos you add from this phone show up on ${tvName}.`)),
      tvCard(tvName, el("span", { class: "ok" }, icon("check", 22)), "Approved on the TV · just now"),
      el("a", { class: "button", href: uploadUrl }, icon("images", 20), el("span", {}, "Choose photos")),
      el("p", { class: "small" }, "Next time, this phone opens straight to your photos."),
    );
    location.replace(uploadUrl);
  };

  const ask = (tvName: string, problem?: string) => {
    const name = el("input", { type: "text", placeholder: "Your name", autocomplete: "given-name", maxlength: "40", value: storage.getItem(NAME_KEY) ?? "" });
    const error = el("p", { class: "error" }, problem ?? "");
    const go = button("Ask to connect");
    go.onclick = async () => {
      if (!name.value.trim()) {
        error.textContent = "Tell the TV who is asking.";
        return;
      }
      go.disabled = true;
      try {
        const asked = await api.requestPairing(code, name.value);
        const waiting = { code, token: asked.token, name: asked.name, tvName: asked.tv_name };
        saveWaiting(storage, waiting);
        await wait(waiting);
      } catch (e) {
        go.disabled = false;
        error.textContent = e instanceof Error ? e.message : String(e);
      }
    };
    mount(
      root,
      topbar(),
      el("div", { class: "stack" }, eyebrow(`Code ${code}`), el("h1", {}, `Connect to ${tvName}`), el("p", { class: "lead" }, "Say who you are. Someone at the TV lets you in, then you add photos. No account, no password.")),
      field("Your name", name),
      error,
      go,
    );
    name.focus();
  };

  const wait = async (waiting: Waiting) => {
    mount(
      root,
      topbar(),
      el("div", { class: "stack" }, eyebrow(`Code ${code}`), el("h1", {}, "Waiting for the TV"), el("p", { class: "lead" }, `${waiting.tvName} is showing your name. Press Allow with the remote.`)),
      tvCard(waiting.tvName, el("span", { class: "pulse", "aria-hidden": "true" }), `Asking as ${waiting.name}`),
      el("p", { class: "small" }, "Keep this page open. It moves on by itself."),
    );
    const outcome = await untilDecided(api, waiting.token);
    if (!outcome) return;
    if (outcome.state === "approved") return connected(outcome.tv_name ?? waiting.tvName, outcome.token);
    clearWaiting(storage);
    ask(waiting.tvName, outcome.state === "denied" ? "Not approved on the TV. Ask again if that was a mistake." : "That took too long. Ask again.");
  };

  // Already paired: straight to the photos.
  const paired = storage.getItem(OWNER_TOKEN_KEY);
  if (paired) {
    try {
      const me = await api.me(paired);
      return connected(me.household_name ?? "your TV", paired);
    } catch {
      storage.removeItem(OWNER_TOKEN_KEY);
    }
  }
  const waiting = readWaiting(storage, code);
  if (waiting) return wait(waiting);
  mount(root, ...page("Finding your TV…"));
  try {
    ask((await api.describePairing(code)).tv_name);
  } catch (e) {
    mount(root, ...page("That code is no longer on the TV", el("p", { class: "lead" }, "Codes change every ten minutes. Scan the one on screen now."), el("p", { class: "error" }, e instanceof Error ? e.message : String(e))));
  }
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
      mount(root, ...page("Link your TV first", el("p", { class: "lead" }, "Scan the code on your own TV first, then scan this invite again.")));
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

  const pairing = url.searchParams.get("p");
  if (pairing) {
    await pair(root, api, pairing.toUpperCase(), config.uploadBase ?? location.origin.replace("link.", "upload."), location, storage);
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
