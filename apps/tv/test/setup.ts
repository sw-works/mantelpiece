// Jest runs under node; give the app a fetch it can stub and quiet RN's animated warnings.
if (typeof globalThis.fetch !== "function") {
  globalThis.fetch = (() => Promise.reject(new Error("fetch not stubbed"))) as typeof fetch;
}
