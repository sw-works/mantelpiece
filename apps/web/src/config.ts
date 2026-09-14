// Deployment values are injected at build time (see infra outputs).

export interface WebConfig {
  apiBase: string;
  cognitoDomain: string;
  cognitoClientId: string;
  /** Origin of the uploader when it is not `upload.` beside `link.` (CloudFront hosting). */
  uploadBase?: string;
}

export function loadConfig(env: Record<string, string | undefined> = import.meta.env): WebConfig {
  const apiBase = env.VITE_API_BASE;
  const cognitoDomain = env.VITE_COGNITO_DOMAIN;
  const cognitoClientId = env.VITE_COGNITO_CLIENT_ID;
  if (!apiBase || !cognitoDomain || !cognitoClientId) {
    throw new Error("set VITE_API_BASE, VITE_COGNITO_DOMAIN and VITE_COGNITO_CLIENT_ID");
  }
  const uploadBase = env.VITE_UPLOAD_BASE?.replace(/\/+$/, "") || undefined;
  return { apiBase, cognitoDomain, cognitoClientId, uploadBase };
}
