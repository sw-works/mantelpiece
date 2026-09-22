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

export type LivePhase = "idle" | "camera" | "starting" | "live" | "stopping" | "ended";

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
