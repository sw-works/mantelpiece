// What the Go live page decides without a camera or a network: which camera, how the
// broadcast is framed, and what to say when something fails.

/** Back camera for the event, front for "wave at Grandma". */
export type Facing = "environment" | "user";

export function cameraConstraints(facing: Facing): MediaStreamConstraints {
  return {
    video: { facingMode: facing, width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30 } },
    audio: { echoCancellation: true, noiseSuppression: true },
  };
}

/** The TV is landscape; a phone held upright still broadcasts landscape, pillarboxed. */
export function holdHint(width: number, height: number): string | null {
  return height > width ? "Turn your phone sideways to fill the TV." : null;
}

/**
 * What `POST /live` carries. Keeping the broadcast sends this phone's zone too (minutes
 * east of UTC), so the saved video lands on the day it happened here.
 */
export function askBody(save: boolean, now: Date = new Date()): { save: boolean; utc_offset_minutes?: number } {
  return save ? { save, utc_offset_minutes: -now.getTimezoneOffset() } : { save };
}

/** The line under the preview while live. */
export function liveLine(tvName: string, saving: boolean): string {
  const line = `Live on ${tvName}. The TV runs a few seconds behind you.`;
  return saving ? `${line} Recording to your memories.` : line;
}

export type LivePhase = "idle" | "camera" | "asking" | "starting" | "live" | "stopping" | "ended";

/** How often a live phone asks whether its broadcast is still the household's. */
export const LIVE_CHECK_MS = 5000;
/** How often a phone waiting on the TV asks for the answer. */
export const ANSWER_CHECK_MS = 2000;

/** The TV ended this broadcast, or another phone took over: `GET /live` no longer names it. */
export function broadcastGone(current: { id: string } | null, mine: string): boolean {
  return current?.id !== mine;
}

/** Why this phone is off the TV: ended there, or the TV switched to someone else. */
export function goneText(current: { id: string; display_name: string } | null, tvName: string): string {
  return current ? `${tvName} switched to ${current.display_name}.` : `${tvName} ended the broadcast.`;
}

/** What to tell the person while asking, or when the TV said anything but yes. */
export function answerText(status: string, tvName: string, replacing: string | null = null): string | null {
  switch (status) {
    case "waiting":
      return replacing ? `Waiting for ${tvName}. ${replacing} is live now; ${tvName} decides whether to switch.` : `Waiting for ${tvName} to let you in…`;
    case "declined":
      return `${tvName} said not now.`;
    case "expired":
      return `${tvName} did not answer. Make sure Mantelpiece is open on it, then try again.`;
    case "cancelled":
      return "Request cancelled.";
    case "ended":
      return "That broadcast has ended.";
    default:
      return null;
  }
}

/** Plain words for the errors this page meets. */
export function liveErrorText(e: unknown): string {
  const name = e instanceof Error ? e.name : "";
  const message = e instanceof Error ? e.message : String(e);
  if (name === "NotAllowedError" || /permission/i.test(message)) return "Camera or microphone access was refused. Allow it in the browser settings and try again.";
  if (name === "NotFoundError") return "No camera found on this device.";
  if (name === "NotReadableError") return "Another app is using the camera.";
  if (/403|forbidden|paired/i.test(message)) return "This phone is not paired with the TV any more. Scan the TV's code again.";
  return `Could not go live: ${message}`;
}
