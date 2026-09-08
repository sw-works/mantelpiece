// Cognito hosted UI, authorization code + PKCE. Login with Amazon is a federated provider
// behind the hosted UI, so the phone never handles Amazon credentials directly.

export interface AuthConfig {
  /** e.g. https://memorylane-123.auth.us-east-1.amazoncognito.com */
  domain: string;
  clientId: string;
  redirectUri: string;
}

export interface Tokens {
  idToken: string;
  accessToken: string;
  expiresAt: number;
}

const VERIFIER_KEY = "ml.pkce.verifier";
const STATE_KEY = "ml.pkce.state";
const TOKENS_KEY = "ml.tokens";

function base64url(bytes: Uint8Array): string {
  let s = "";
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function randomString(bytes = 32): string {
  const buf = new Uint8Array(bytes);
  crypto.getRandomValues(buf);
  return base64url(buf);
}

export async function challengeFor(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  return base64url(new Uint8Array(digest));
}

/** Where to send the browser. Stores the verifier and state for the callback. */
export async function beginSignIn(
  config: AuthConfig,
  storage: Storage,
  returnTo: string,
  identityProvider = "LoginWithAmazon",
): Promise<string> {
  const verifier = randomString();
  const state = randomString(16);
  storage.setItem(VERIFIER_KEY, verifier);
  storage.setItem(STATE_KEY, JSON.stringify({ state, returnTo }));
  const params = new URLSearchParams({
    response_type: "code",
    client_id: config.clientId,
    redirect_uri: config.redirectUri,
    scope: "openid profile email",
    state,
    code_challenge: await challengeFor(verifier),
    code_challenge_method: "S256",
    identity_provider: identityProvider,
  });
  return `${config.domain}/oauth2/authorize?${params.toString()}`;
}

/** Handle `/callback?code=&state=`. Returns tokens and where the flow started. */
export async function completeSignIn(
  config: AuthConfig,
  storage: Storage,
  callbackUrl: string,
  fetchFn: typeof fetch = fetch,
): Promise<{ tokens: Tokens; returnTo: string }> {
  const url = new URL(callbackUrl);
  const code = url.searchParams.get("code");
  const state = url.searchParams.get("state");
  const saved = storage.getItem(STATE_KEY);
  const verifier = storage.getItem(VERIFIER_KEY);
  if (!code || !state || !saved || !verifier) throw new Error("sign-in did not start here");
  const { state: expected, returnTo } = JSON.parse(saved) as { state: string; returnTo: string };
  if (state !== expected) throw new Error("sign-in state mismatch");
  storage.removeItem(STATE_KEY);
  storage.removeItem(VERIFIER_KEY);

  const body = new URLSearchParams({
    grant_type: "authorization_code",
    client_id: config.clientId,
    code,
    redirect_uri: config.redirectUri,
    code_verifier: verifier,
  });
  const res = await fetchFn(`${config.domain}/oauth2/token`, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body,
  });
  if (!res.ok) throw new Error(`token exchange failed: ${res.status}`);
  const json = (await res.json()) as { id_token: string; access_token: string; expires_in: number };
  const tokens: Tokens = {
    idToken: json.id_token,
    accessToken: json.access_token,
    expiresAt: Date.now() + json.expires_in * 1000,
  };
  storage.setItem(TOKENS_KEY, JSON.stringify(tokens));
  return { tokens, returnTo };
}

export function savedTokens(storage: Storage, now = Date.now()): Tokens | null {
  const raw = storage.getItem(TOKENS_KEY);
  if (!raw) return null;
  const tokens = JSON.parse(raw) as Tokens;
  return tokens.expiresAt > now + 30_000 ? tokens : null;
}
