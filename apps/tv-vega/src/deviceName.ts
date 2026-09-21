// What this TV calls itself, offered as the household name on first launch. The
// system-deployed identifiers module answers with the model name ("Fire TV Stick 4K")
// unless the app holds the device-friendly-name run-time privilege, which would mean a
// consent dialog before the first screen; the person can change the name on the TV instead.
// Falls back when the module is missing (jest, other platforms) or slow.

export const FALLBACK_NAME = "Fire TV";

interface Identifiers {
  getFriendlyDeviceName?: () => Promise<unknown>;
}

function identifiers(): Identifiers | null {
  try {
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const mod = require("@amazon-devices/kepler-identifiers") as { KeplerIdentifiers?: Identifiers };
    return mod.KeplerIdentifiers ?? null;
  } catch {
    return null;
  }
}

/** A bare device-type code such as "AQVV01P" (what the Virtual Device answers) names nothing. */
export function looksLikeModelCode(name: string): boolean {
  return /^[A-Z0-9]{5,14}$/.test(name) && /\d/.test(name);
}

export async function deviceName(source: Identifiers | null = identifiers(), timeoutMs = 1500): Promise<string> {
  if (!source?.getFriendlyDeviceName) return FALLBACK_NAME;
  try {
    const name = await Promise.race([source.getFriendlyDeviceName(), new Promise<null>((r) => setTimeout(() => r(null), timeoutMs))]);
    const clean = typeof name === "string" ? name.trim().slice(0, 40) : "";
    return clean.length > 0 && !looksLikeModelCode(clean) ? clean : FALLBACK_NAME;
  } catch {
    return FALLBACK_NAME;
  }
}
