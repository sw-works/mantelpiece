// live.html: this phone broadcasts to the household's TV. The camera opens first so the
// person sees what the TV will see; "Go live" asks the TV, which grants or declines. Only a
// grant makes a channel and hands this phone the stream key for the IVS broadcast SDK
// (WebRTC ingest). The SDK loads only on this page. "Save to memories", off unless the
// person turns it on, asks for a recorded channel; the video joins the household's
// memories a few minutes after the broadcast ends.

import { Api } from "../api.js";
import { loadConfig } from "../config.js";
import { phoneToken } from "../family.js";
import { ANSWER_CHECK_MS, LIVE_CHECK_MS, answerText, askBody, broadcastGone, cameraConstraints, goneText, holdHint, liveErrorText, liveLine, type Facing, type LivePhase } from "../live.js";
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
  const keep = el("input", { type: "checkbox", id: "keep" });
  const keepRow = el("label", { class: "keep", for: "keep" }, keep, el("span", {}, "Save to memories"));

  let phase: LivePhase = "idle";
  let facing: Facing = "environment";
  let sdk: BroadcastSdk | null = null;
  let client: BroadcastClient | null = null;
  let stream: MediaStream | null = null;
  let check: ReturnType<typeof setInterval> | undefined;
  let waiting: ReturnType<typeof setInterval> | undefined;
  const cancel = button("Cancel", { secondary: true });
  cancel.hidden = true;

  const set = (next: LivePhase, text?: string) => {
    phase = next;
    if (next !== "live" && check) {
      clearInterval(check);
      check = undefined;
    }
    if (next !== "asking" && waiting) {
      clearInterval(waiting);
      waiting = undefined;
    }
    cancel.hidden = next !== "asking";
    primary.disabled = next === "asking" || next === "starting" || next === "stopping";
    flip.hidden = !(next === "camera" || next === "live");
    // Chosen before asking; the channel is made recorded or not when the TV says yes.
    keepRow.hidden = !(next === "idle" || next === "camera" || next === "ended");
    const label = { idle: "Open the camera", camera: `Go live on ${tvName}`, asking: `Asking ${tvName}…`, starting: "Starting…", live: "Stop", stopping: "Stopping…", ended: "Go live again" }[next];
    primary.replaceChildren(icon(next === "live" ? "check" : "video", 20), el("span", {}, label));
    primary.classList.toggle("secondary", next === "live");
    status.textContent = text ?? { idle: "Nothing is sent until you press Go live.", camera: "Only you can see this preview.", asking: `Waiting for ${tvName} to let you in…`, starting: "Connecting…", live: `Live on ${tvName}. The TV runs a few seconds behind you.`, stopping: "", ended: "Broadcast ended." }[next];
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

  // The TV's answer: stream on a grant, say why on anything else.
  const answered = async () => {
    if (phase !== "asking") return;
    const answer = await api.liveAnswer(token).catch(() => null);
    if (!answer || phase !== "asking" || answer.status === "waiting") return;
    if (answer.status !== "granted" || !answer.stream_key || !answer.ingest_endpoint || !answer.broadcast) {
      set("camera", answerText(answer.status, tvName) ?? undefined);
      return;
    }
    const mine = answer.broadcast;
    set("starting");
    try {
      await client!.startBroadcast(answer.stream_key, answer.ingest_endpoint);
    } catch (e) {
      await api.stopLive(token).catch(() => undefined);
      set("camera", liveErrorText(e));
      return;
    }
    set("live", liveLine(tvName, answer.saving === true));
    // The TV can end it or switch to another phone; the SDK does not always notice the cut.
    check = setInterval(() => {
      void api
        .currentLive(token)
        .then(({ live: now }) => {
          if (phase !== "live" || !broadcastGone(now, mine)) return;
          client?.stopBroadcast();
          set("ended", goneText(now, tvName));
        })
        .catch(() => undefined);
    }, LIVE_CHECK_MS);
  };

  cancel.onclick = async () => {
    set("camera", answerText("cancelled", tvName) ?? undefined);
    await api.cancelLive(token).catch(() => undefined);
  };

  primary.onclick = async () => {
    try {
      if (phase === "idle" || phase === "ended") {
        if (!stream) await openCamera();
        set("camera");
        return;
      }
      if (phase === "camera") {
        const asked = await api.askLive(token, askBody(keep.checked));
        const line = answerText("waiting", tvName, asked.replacing) ?? "";
        set("asking", keep.checked && !asked.saving ? `${line} This TV cannot save broadcasts, so it will only show it.` : line);
        waiting = setInterval(() => void answered(), ANSWER_CHECK_MS);
        return;
      }
      if (phase === "live") await stop();
    } catch (e) {
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
    if (phase === "asking") void api.cancelLive(token).catch(() => undefined);
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
    keepRow,
    cancel,
    flip,
    status,
    el("a", { class: "link", href: "/?owner=1" }, icon("back", 18), el("span", {}, "Back to photos")),
  );
  set("idle");
  updateHint();
  void location;
}
