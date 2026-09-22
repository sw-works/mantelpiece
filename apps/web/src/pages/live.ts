// live.html: this phone broadcasts to the household's TV. The camera opens first so the
// person sees what the TV will see; "Go live" asks the API for a channel and starts the
// IVS broadcast SDK (WebRTC ingest). The SDK loads only on this page.

import { Api } from "../api.js";
import { loadConfig } from "../config.js";
import { phoneToken } from "../family.js";
import { cameraConstraints, holdHint, liveErrorText, type Facing, type LivePhase } from "../live.js";
import { button, context, el, icon, mount, topbar } from "../ui.js";
import { deviceIdOf } from "./upload.js";

type BroadcastSdk = typeof import("amazon-ivs-web-broadcast").default;
type BroadcastClient = ReturnType<BroadcastSdk["create"]>;

export async function main(root: HTMLElement, location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase, fetch, deviceIdOf(storage));
  const token = phoneToken(storage);
  if (!token) {
    mount(root, topbar(), el("div", { class: "stack" }, el("h1", {}, "Pair this phone first"), el("p", { class: "lead" }, "Scan the code on the TV, then come back here to go live.")));
    return;
  }
  const me = await api.me(token).catch(() => null);
  const tvName = me?.household_name ?? "the TV";

  const preview = el("canvas", { class: "preview" });
  const status = el("p", { class: "meta" });
  const hint = el("p", { class: "small dim" });
  const primary = button("Open the camera", { icon: "video" });
  const flip = button("Switch camera", { secondary: true });
  flip.hidden = true;

  let phase: LivePhase = "idle";
  let facing: Facing = "environment";
  let sdk: BroadcastSdk | null = null;
  let client: BroadcastClient | null = null;
  let stream: MediaStream | null = null;

  const set = (next: LivePhase, text?: string) => {
    phase = next;
    primary.disabled = next === "starting" || next === "stopping";
    flip.hidden = !(next === "camera" || next === "live");
    const label = { idle: "Open the camera", camera: `Go live on ${tvName}`, starting: "Starting…", live: "Stop", stopping: "Stopping…", ended: "Go live again" }[next];
    primary.replaceChildren(icon(next === "live" ? "check" : "video", 20), el("span", {}, label));
    primary.classList.toggle("secondary", next === "live");
    status.textContent = text ?? { idle: "Nothing is sent until you press Go live.", camera: "Only you can see this preview.", starting: "Connecting…", live: `Live on ${tvName}. The TV runs a few seconds behind you.`, stopping: "", ended: "Broadcast ended." }[next];
  };

  const updateHint = () => {
    hint.textContent = holdHint(window.innerWidth, window.innerHeight) ?? "";
  };
  window.addEventListener("resize", updateHint);

  const openCamera = async () => {
    stream?.getTracks().forEach((t) => t.stop());
    stream = await navigator.mediaDevices.getUserMedia(cameraConstraints(facing));
    sdk ??= (await import("amazon-ivs-web-broadcast")).default;
    if (!client) {
      client = sdk.create({ streamConfig: sdk.BASIC_LANDSCAPE });
      client.attachPreview(preview);
      client.on(sdk.BroadcastClientEvents.CONNECTION_STATE_CHANGE, (...args: unknown[]) => {
        const state = String(args[0]);
        if (phase === "live" && (state === "failed" || state === "disconnected")) set("camera", "The connection dropped. Press Go live to try again.");
      });
    }
    // Swapping cameras: the SDK throws when a name is not attached yet.
    for (const remove of [() => client!.removeVideoInputDevice("camera"), () => client!.removeAudioInputDevice("mic")]) {
      try {
        remove();
      } catch {
        // nothing attached under that name yet
      }
    }
    await client.addVideoInputDevice(new MediaStream(stream.getVideoTracks()), "camera", { index: 0 });
    await client.addAudioInputDevice(new MediaStream(stream.getAudioTracks()), "mic");
  };

  const stop = async () => {
    set("stopping");
    client?.stopBroadcast();
    await api.stopLive(token).catch(() => undefined);
    set("ended");
  };

  primary.onclick = async () => {
    try {
      if (phase === "idle" || phase === "ended") {
        if (!stream) await openCamera();
        set("camera");
        return;
      }
      if (phase === "camera") {
        set("starting");
        const live = await api.startLive(token);
        await client!.startBroadcast(live.stream_key, live.ingest_endpoint);
        set("live");
        return;
      }
      if (phase === "live") await stop();
    } catch (e) {
      if (phase === "starting") await api.stopLive(token).catch(() => undefined);
      set(stream ? "camera" : "idle", liveErrorText(e));
    }
  };
  flip.onclick = async () => {
    facing = facing === "environment" ? "user" : "environment";
    try {
      await openCamera();
    } catch (e) {
      status.textContent = liveErrorText(e);
    }
  };
  // Leaving the page ends the broadcast rather than leaving the TV on a frozen frame.
  window.addEventListener("pagehide", () => {
    if (phase === "live") {
      client?.stopBroadcast();
      void api.stopLive(token).catch(() => undefined);
    }
  });

  mount(
    root,
    topbar({ right: context(`Live on ${tvName}`) }),
    el("div", { class: "stack" }, el("h1", {}, "Show them the moment"), el("p", { class: "lead" }, `Your camera on ${tvName}, a few seconds later. Everyone there watches with you.`)),
    preview,
    hint,
    primary,
    flip,
    status,
    el("a", { class: "link", href: "/?owner=1" }, icon("back", 18), el("span", {}, "Back to photos")),
  );
  set("idle");
  updateHint();
  void location;
}
