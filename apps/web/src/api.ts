// Thin client over the backend API. Bearer is an app session token (owner phone or
// upload session); `jwt` is the Cognito id token for the two JWT-guarded routes.

export interface CompletedPart {
  part_number: number;
  etag: string;
}

export interface InitiatedUpload {
  media_id: string;
  upload_id: string;
  key: string;
  part_size: number;
  part_urls: string[];
}

export interface Joined {
  token: string;
  household: string;
  contributor: string;
  display_name: string;
  trip: string | null;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export class Api {
  constructor(
    readonly base: string,
    private readonly fetchFn: typeof fetch = fetch,
  ) {}

  private async call<T>(
    method: "GET" | "POST" | "DELETE",
    path: string,
    body?: unknown,
    auth?: { bearer?: string; jwt?: string },
  ): Promise<T> {
    const headers: Record<string, string> = {};
    if (body !== undefined) headers["content-type"] = "application/json";
    if (auth?.bearer) headers["authorization"] = `Bearer ${auth.bearer}`;
    if (auth?.jwt) headers["authorization"] = `Bearer ${auth.jwt}`;
    const res = await this.fetchFn(`${this.base}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (res.status === 204) return undefined as T;
    const json = (await res.json().catch(() => ({}))) as { error?: string };
    if (!res.ok) throw new ApiError(res.status, json.error ?? `HTTP ${res.status}`);
    return json as T;
  }

  approveDeviceCode(code: string, jwt: string) {
    return this.call<{ approved: boolean; household: string; token: string }>("POST", "/device/approve", { code }, { jwt });
  }

  joinAsGuest(code: string, displayName: string) {
    return this.call<Joined>("POST", "/join", { code, display_name: displayName });
  }

  joinWithAmazon(code: string, jwt: string) {
    return this.call<Joined>("POST", "/join/amazon", { code }, { jwt });
  }

  me(bearer: string) {
    return this.call<{ role: string; display_name?: string; trip?: string | null }>("GET", "/me", undefined, { bearer });
  }

  initUpload(bearer: string, file: { filename: string; bytes: number; sha256: string; content_type: string; batch_id: string }) {
    return this.call<InitiatedUpload>("POST", "/uploads/init", file, { bearer });
  }

  completeUpload(
    bearer: string,
    body: { media_id: string; upload_id: string; parts: CompletedPart[]; batch_done: number; batch_total: number },
  ) {
    return this.call<{ media_id: string }>("POST", "/uploads/complete", body, { bearer });
  }

  abortUpload(bearer: string, media_id: string, upload_id: string) {
    return this.call<void>("POST", "/uploads/abort", { media_id, upload_id }, { bearer });
  }

  deleteMedia(bearer: string, mediaId: string) {
    return this.call<void>("DELETE", `/media/${mediaId}`, undefined, { bearer });
  }

  trips(bearer: string) {
    return this.call<Array<{ id: string; name: string; start: string; end: string; highlights: string[] }>>("GET", "/trips", undefined, { bearer });
  }

  mediaUrl(bearer: string, mediaId: string, rendition: "thumb" | "tv") {
    return this.call<{ url: string }>("GET", `/media/${mediaId}/url?rendition=${rendition}`, undefined, { bearer });
  }
}
