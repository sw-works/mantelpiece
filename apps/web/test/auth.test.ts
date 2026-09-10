import { describe, expect, it } from "vitest";
import { beginSignIn, challengeFor, completeSignIn, ownerHandoffUrl, savedTokens, takeOwnerHandoff } from "../src/auth.js";

class MemStorage implements Storage {
  private m = new Map<string, string>();
  get length() {
    return this.m.size;
  }
  clear() {
    this.m.clear();
  }
  getItem(k: string) {
    return this.m.get(k) ?? null;
  }
  key(i: number) {
    return [...this.m.keys()][i] ?? null;
  }
  removeItem(k: string) {
    this.m.delete(k);
  }
  setItem(k: string, v: string) {
    this.m.set(k, v);
  }
}

const config = { domain: "https://auth.test", clientId: "client", redirectUri: "https://link.test/callback" };

describe("PKCE sign-in", () => {
  it("builds an S256 challenge per RFC 7636 appendix B", async () => {
    expect(await challengeFor("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")).toBe(
      "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
    );
  });

  it("round-trips authorize and callback", async () => {
    const storage = new MemStorage();
    const url = new URL(await beginSignIn(config, storage, "https://link.test/?c=ABC"));
    expect(url.origin + url.pathname).toBe("https://auth.test/oauth2/authorize");
    expect(url.searchParams.get("identity_provider")).toBe("LoginWithAmazon");
    expect(url.searchParams.get("code_challenge_method")).toBe("S256");
    const state = url.searchParams.get("state")!;

    let tokenBody = "";
    const fetchFn = (async (_input: RequestInfo | URL, init?: RequestInit) => {
      tokenBody = String(init?.body);
      return new Response(JSON.stringify({ id_token: "id", access_token: "at", expires_in: 3600 }), { status: 200 });
    }) as typeof fetch;
    const { tokens, returnTo } = await completeSignIn(config, storage, `https://link.test/callback?code=xyz&state=${state}`, fetchFn);
    expect(returnTo).toBe("https://link.test/?c=ABC");
    expect(tokens.idToken).toBe("id");
    expect(new URLSearchParams(tokenBody).get("code")).toBe("xyz");
    expect(new URLSearchParams(tokenBody).get("code_verifier")).toBeTruthy();
    expect(savedTokens(storage)?.idToken).toBe("id");
    expect(savedTokens(storage, Date.now() + 3600_000)).toBeNull();
    await expect(completeSignIn(config, storage, `https://link.test/callback?code=xyz&state=${state}`, fetchFn)).rejects.toThrow(
      "did not start here",
    );
  });

  it("hands the owner session to the uploader in the fragment only", () => {
    const url = ownerHandoffUrl("https://upload.test", "a.b/c=");
    expect(url).toBe("https://upload.test/?owner=1#owner=a.b%2Fc%3D");
    expect(new URL(url).search).toBe("?owner=1");
    expect(takeOwnerHandoff(new URL(url).hash)).toBe("a.b/c=");
    expect(takeOwnerHandoff("")).toBeNull();
    expect(takeOwnerHandoff("#owner=")).toBeNull();
    expect(takeOwnerHandoff("#other=1")).toBeNull();
  });

  it("rejects a state mismatch", async () => {
    const storage = new MemStorage();
    await beginSignIn(config, storage, "https://link.test/");
    await expect(completeSignIn(config, storage, "https://link.test/callback?code=x&state=wrong")).rejects.toThrow("mismatch");
  });
});
