// upload.<domain>/ask.html: hold to talk, or type, and the answer shows on the TV (F-405).

import { Api } from "../api.js";
import { describeOutcome, nowShowingLine, speechRecognition, transcriptOf } from "../ask.js";
import { OWNER_TOKEN_KEY } from "../auth.js";
import { loadConfig } from "../config.js";
import { el, mount } from "../ui.js";

export async function main(root: HTMLElement, _location: Location, storage: Storage): Promise<void> {
  const config = loadConfig();
  const api = new Api(config.apiBase);
  const token = storage.getItem(OWNER_TOKEN_KEY);
  if (!token) {
    mount(root, el("h1", {}, "Ask about your photos"), el("p", {}, "Sign in your TV first: scan the code on the TV with this phone."));
    return;
  }
  const status = el("p", { class: "muted" }, "Checking what is on the TV…");
  const answer = el("p", { class: "story" });
  const input = el("input", { type: "text", placeholder: "What's that statue in the background?" });
  const send = el("button", { class: "button" }, "Ask");
  const Recognition = speechRecognition(window as unknown as Record<string, unknown>);
  const talk = el("button", { class: `button ${Recognition ? "" : "secondary"}` }, Recognition ? "Hold to talk" : "Voice needs Safari or Chrome; type instead");
  talk.disabled = !Recognition;

  let tv: string | undefined;
  api
    .presence(token)
    .then((list) => {
      const freshest = [...list].sort((a, b) => b.at.localeCompare(a.at))[0];
      tv = freshest?.device_id;
      status.textContent = nowShowingLine(freshest);
    })
    .catch(() => (status.textContent = nowShowingLine(undefined)));

  const ask = async (question: string) => {
    if (!question.trim()) return;
    send.disabled = true;
    answer.textContent = "Asking…";
    try {
      answer.textContent = describeOutcome(await api.ask(token, question.trim(), tv));
    } catch (e) {
      answer.textContent = e instanceof Error ? e.message : String(e);
    } finally {
      send.disabled = false;
    }
  };
  send.onclick = () => void ask(input.value);
  input.onkeydown = (e) => {
    if (e.key === "Enter") void ask(input.value);
  };
  if (Recognition) {
    let active: InstanceType<typeof Recognition> | null = null;
    const start = () => {
      active = new Recognition();
      active.lang = navigator.language || "en-US";
      active.interimResults = false;
      active.maxAlternatives = 1;
      active.onresult = (event) => {
        input.value = transcriptOf(event);
        void ask(input.value);
      };
      active.onerror = (event) => (answer.textContent = `Could not hear you (${event.error}).`);
      active.onend = () => (active = null);
      active.start();
      talk.textContent = "Listening… release to ask";
    };
    const stop = () => {
      active?.stop();
      talk.textContent = "Hold to talk";
    };
    talk.onpointerdown = start;
    talk.onpointerup = stop;
    talk.onpointercancel = stop;
  }
  mount(root, el("h1", {}, "Ask about your photos"), status, talk, input, send, answer);
}
