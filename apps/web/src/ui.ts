// Tiny DOM helpers; no framework. The logo, icons, top bar and the few shared
// components every page uses live here so the pages look like one product.

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else node.setAttribute(k, v);
  }
  node.append(...children);
  return node;
}

export function mount(root: HTMLElement, ...children: (Node | string)[]): void {
  root.replaceChildren(...children);
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

// Inline SVG icons, stroke 2 on a 24 viewBox. Never emoji.
const ICONS = {
  memories: '<path d="M4 6h16v12H4z"/><path d="M4 15l4-4 4 4 3-3 5 5"/><circle cx="16" cy="9" r="1.5"/>',
  people: '<circle cx="9" cy="8" r="3.5"/><path d="M2.5 20a6.5 6.5 0 0 1 13 0"/><circle cx="17" cy="9" r="2.5"/><path d="M15 20a5 5 0 0 1 6.5-4.6"/>',
  ask: '<circle cx="11" cy="11" r="7"/><path d="M16 16l5 5"/>',
  play: '<path d="M7 4l14 8-14 8z" fill="currentColor" stroke="none"/>',
  tv: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>',
  back: '<path d="M14 6l-6 6 6 6"/>',
  heart: '<path d="M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z"/>',
  check: '<path d="M5 12l4 4L19 6"/>',
  mic: '<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0 0 14 0M12 18v3"/>',
  images: '<rect x="3" y="7" width="14" height="12" rx="2"/><path d="M7 3h14v12"/><path d="M3 16l4-4 3 3 2-2 5 5"/>',
  share: '<path d="M12 3v12M7 8l5-5 5 5"/><path d="M5 13v6h14v-6"/>',
  retry: '<path d="M20 12a8 8 0 1 1-2.3-5.7"/><path d="M20 4v5h-5"/>',
  amazon: '<path d="M4 15c4 3 12 3 16 0"/><path d="M18 14l2 1-1 2"/><path d="M9 11a3 3 0 1 1 6 0v3"/>',
  chev: '<path d="M9 6l6 6-6 6"/>',
  close: '<path d="M7 7l10 10M17 7L7 17"/>',
  video: '<rect x="3" y="6" width="13" height="12" rx="2"/><path d="M16 10l5-3v10l-5-3z"/>',
} as const;

export type IconName = keyof typeof ICONS;

function fromMarkup(markup: string): SVGSVGElement {
  const t = document.createElement("template");
  t.innerHTML = markup;
  return t.content.firstElementChild as SVGSVGElement;
}

export function icon(name: IconName, size = 22): SVGSVGElement {
  return fromMarkup(
    `<svg width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name]}</svg>`,
  );
}

/** The route mark: an amber polyline with three stops on a rounded square. */
export function logo(size = 32): SVGSVGElement {
  const dot = (x: number, y: number) => `<circle cx="${x}" cy="${y}" r="4" fill="#f2f2f5" stroke="#0b0b0f" stroke-width="2"/>`;
  return fromMarkup(
    `<svg width="${size}" height="${size}" viewBox="0 0 44 44" fill="none" aria-hidden="true"><rect width="44" height="44" rx="10" fill="#16161c"/><path d="M9 34l9-12 6 5 11-16" stroke="#ffb454" stroke-width="4" stroke-linecap="round" stroke-linejoin="round"/>${dot(9, 34)}${dot(24, 27)}${dot(35, 11)}</svg>`,
  );
}

/**
 * The 44px row at the top of every page: logo and wordmark on the left (a back arrow
 * instead when `back` is a URL), and an optional slot on the right for the context chip.
 */
export function topbar(opts: { right?: Node; back?: string } = {}): HTMLElement {
  const wordmark = el("span", { class: "wordmark" }, "Mantelpiece");
  const brand = opts.back ? el("a", { class: "brand", href: opts.back, "aria-label": "Back" }, icon("back"), wordmark) : el("span", { class: "brand" }, logo(), wordmark);
  return el("header", { class: "topbar" }, brand, opts.right ?? el("span"));
}

/** "Your household · as You": the pill that says where photos go and who is adding. */
export function context(text: string): HTMLElement {
  return el("span", { class: "context" }, el("span", {}, text));
}

export function eyebrow(text: string): HTMLElement {
  return el("p", { class: "eyebrow" }, text);
}

/** A 56px pill. Primary is amber; `secondary` is the quiet one. */
export function button(label: string, opts: { secondary?: boolean; icon?: IconName; inline?: boolean } = {}): HTMLButtonElement {
  const cls = ["button", opts.secondary ? "secondary" : "", opts.inline ? "inline" : ""].filter(Boolean).join(" ");
  return el("button", { class: cls, type: "button" }, ...(opts.icon ? [icon(opts.icon, 20)] : []), el("span", {}, label));
}

/** A text link with a tap target; a `<button>` unless `href` is given. */
export function link(label: string, opts: { icon?: IconName; href: string; small?: boolean }): HTMLAnchorElement;
export function link(label: string, opts?: { icon?: IconName; href?: undefined; small?: boolean }): HTMLButtonElement;
export function link(label: string, opts: { icon?: IconName; href?: string; small?: boolean } = {}): HTMLButtonElement | HTMLAnchorElement {
  const cls = opts.small ? "link small" : "link";
  const kids: (Node | string)[] = [...(opts.icon ? [icon(opts.icon, 16)] : []), label];
  return opts.href ? el("a", { class: cls, href: opts.href }, ...kids) : el("button", { class: cls, type: "button" }, ...kids);
}

/** A labelled input. */
export function field(label: string, input: HTMLElement, small = false): HTMLElement {
  return el("label", { class: "field" }, el("span", { class: small ? "label small" : "label" }, label), input);
}

/**
 * A thumbnail box that fills in when the URL arrives and stays a neutral panel when it
 * does not. `cls` picks the size: hero, round, small, wide, tile, strip.
 */
export function photo(src: Promise<string> | string, cls = ""): HTMLElement {
  const img = el("img", { alt: "", loading: "lazy" });
  const box = el("div", { class: cls ? `thumb ${cls}` : "thumb" }, img);
  if (typeof src === "string") img.src = src;
  else src.then((u) => (img.src = u)).catch(() => img.remove());
  return box;
}
