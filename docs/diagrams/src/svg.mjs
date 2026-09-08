// Tiny SVG diagram builder: typed nodes on a grid, labelled edges, one legend.
// No layout engine — every diagram file places its nodes explicitly so the
// picture is deterministic and reviewable as text.

export const KIND = {
  det:      { fill: "#f6f8fa", stroke: "#57606a", dash: "",     label: "deterministic code" },
  llm:      { fill: "#eef2ff", stroke: "#4c5fd5", dash: "",     label: "LLM node" },
  human:    { fill: "#fff8e6", stroke: "#b08500", dash: "",     label: "human decision" },
  store:    { fill: "#ffffff", stroke: "#57606a", dash: "5 3",  label: "store / queue" },
  ext:      { fill: "#f3f4f6", stroke: "#8c959f", dash: "2 3",  label: "external system" },
  security: { fill: "#fff0f0", stroke: "#cf222e", dash: "",     label: "security service" },
  state:    { fill: "#ffffff", stroke: "#57606a", dash: "",     label: "state" },
  terminal: { fill: "#e6f4ea", stroke: "#1a7f37", dash: "",     label: "terminal / outcome" },
  blocked:  { fill: "#fff0f0", stroke: "#cf222e", dash: "",     label: "blocked / failed" },
};

const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

export function diagram({ width, height, title, subtitle, nodes, edges, groups = [], legend = [], notes = [] }) {
  const byId = Object.fromEntries(nodes.map((n) => [n.id, n]));
  const out = [];
  out.push(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}" font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif" font-size="12" role="img" aria-label="${esc(title)}">`);
  out.push(`<defs>
  <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="#24292f"/></marker>
  <marker id="arrow-muted" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="#8c959f"/></marker>
  <marker id="arrow-red" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="#cf222e"/></marker>
</defs>`);
  out.push(`<rect width="${width}" height="${height}" fill="#ffffff"/>`);
  out.push(`<text x="24" y="34" font-size="20" font-weight="600" fill="#24292f">${esc(title)}</text>`);
  if (subtitle) out.push(`<text x="24" y="54" font-size="12" fill="#57606a">${esc(subtitle)}</text>`);

  for (const g of groups) {
    out.push(`<rect x="${g.x}" y="${g.y}" width="${g.w}" height="${g.h}" rx="10" fill="${g.fill ?? "#fafbfc"}" stroke="${g.stroke ?? "#d0d7de"}" stroke-dasharray="6 4"/>`);
    out.push(`<text x="${g.x + 12}" y="${g.y + 18}" font-size="11" font-weight="600" fill="#57606a" letter-spacing="0.06em">${esc(g.label.toUpperCase())}</text>`);
  }

  // Edges first so nodes paint over them.
  for (const e of edges) {
    const a = byId[e.from], b = byId[e.to];
    if (!a || !b) throw new Error(`edge references unknown node: ${e.from} -> ${e.to}`);
    const pts = e.points ?? straight(a, b, e.side);
    const d = pts.map((p, i) => `${i ? "L" : "M"}${p[0]},${p[1]}`).join(" ");
    const stroke = e.color === "red" ? "#cf222e" : e.muted ? "#8c959f" : "#24292f";
    const marker = e.color === "red" ? "arrow-red" : e.muted ? "arrow-muted" : "arrow";
    out.push(`<path d="${d}" fill="none" stroke="${stroke}" stroke-width="${e.width ?? 1.5}" ${e.dash ? `stroke-dasharray="${e.dash}"` : ""} marker-end="url(#${marker})"/>`);
    if (e.label) {
      const lines = String(e.label).split("\n");
      const w = Math.max(...lines.map((l) => l.length)) * 6.2 + 10, h = lines.length * 14 + 4;
      let [lx, ly] = e.at ?? mid(pts);
      if (!e.at && pts.length === 2) {
        const horizontal = Math.abs(pts[1][1] - pts[0][1]) < 1;
        const len = Math.hypot(pts[1][0] - pts[0][0], pts[1][1] - pts[0][1]);
        if (horizontal && len < w + 12) ly = Math.min(a.y, b.y) - h / 2 - 4;      // above both boxes
        else if (!horizontal && len < h + 12) lx = Math.max(a.x + a.w, b.x + b.w) + w / 2 + 4; // beside both boxes
        else if (horizontal) ly -= h / 2 + 3;                                       // hover above the line
        else lx += w / 2 + 4;                                                       // sit right of the line
      }
      out.push(`<rect x="${lx - w / 2}" y="${ly - h / 2}" width="${w}" height="${h}" rx="3" fill="#ffffff" fill-opacity="0.92"/>`);
      lines.forEach((l, i) => out.push(`<text x="${lx}" y="${ly - h / 2 + 12 + i * 14}" text-anchor="middle" font-size="10.5" fill="${stroke}">${esc(l)}</text>`));
    }
  }

  for (const n of nodes) {
    const k = KIND[n.kind ?? "det"];
    const rx = n.kind === "state" || n.kind === "terminal" || n.kind === "blocked" ? n.h / 2 : n.kind === "store" ? 4 : 6;
    if (n.kind === "human") {
      // diamond-ish decision: keep rect but add a small flag
      out.push(`<rect x="${n.x}" y="${n.y}" width="${n.w}" height="${n.h}" rx="${rx}" fill="${k.fill}" stroke="${k.stroke}" stroke-width="1.5"/>`);
    } else {
      out.push(`<rect x="${n.x}" y="${n.y}" width="${n.w}" height="${n.h}" rx="${rx}" fill="${k.fill}" stroke="${k.stroke}" stroke-width="1.5" ${k.dash ? `stroke-dasharray="${k.dash}"` : ""}/>`);
    }
    const lines = String(n.label).split("\n");
    const sub = n.sub ? String(n.sub).split("\n").filter((l) => l.length) : [];
    // Shrink rather than overflow: bold label ≈ 6.9px/char at 12px, mono sub ≈ 6.1px/char at 10px.
    const avail = n.w - 14;
    const fl = Math.max(0.7, Math.min(1, avail / (Math.max(...lines.map((l) => l.length)) * 6.9)));
    const fs = sub.length ? Math.max(0.72, Math.min(1, avail / (Math.max(...sub.map((l) => l.length)) * 6.1))) : 1;
    const lh = 14 * fl, sh = 12 * fs;
    const total = lines.length * lh + sub.length * sh;
    let y = n.y + n.h / 2 - total / 2 + 11 * fl;
    for (const l of lines) { out.push(`<text x="${n.x + n.w / 2}" y="${y.toFixed(1)}" text-anchor="middle" font-size="${(12 * fl).toFixed(1)}" font-weight="600" fill="#24292f">${esc(l)}</text>`); y += lh; }
    for (const l of sub) { out.push(`<text x="${n.x + n.w / 2}" y="${y.toFixed(1)}" text-anchor="middle" font-size="${(10 * fs).toFixed(1)}" fill="#57606a" font-family="ui-monospace, SFMono-Regular, Menlo, monospace">${esc(l)}</text>`); y += sh; }
    if (n.badge) out.push(`<text x="${n.x + n.w - 6}" y="${n.y + 12}" text-anchor="end" font-size="9" font-weight="600" fill="${k.stroke}">${esc(n.badge)}</text>`);
  }

  if (legend.length) {
    let lx = 24, ly = height - 26;
    for (const kind of legend) {
      const k = KIND[kind];
      out.push(`<rect x="${lx}" y="${ly - 9}" width="18" height="12" rx="3" fill="${k.fill}" stroke="${k.stroke}" ${k.dash ? `stroke-dasharray="${k.dash}"` : ""}/>`);
      out.push(`<text x="${lx + 24}" y="${ly + 1}" font-size="11" fill="#57606a">${esc(k.label)}</text>`);
      lx += 30 + k.label.length * 6.4 + 22;
    }
  }
  notes.forEach((t, i) => out.push(`<text x="${width - 24}" y="${height - 50 - (notes.length - 1 - i) * 14}" text-anchor="end" font-size="10.5" fill="#57606a">${esc(t)}</text>`));
  out.push(`</svg>`);
  return out.join("\n");
}

// Straight edge between nearest sides, with a 6px gap so the arrowhead is not under the box.
function straight(a, b, side) {
  const ac = [a.x + a.w / 2, a.y + a.h / 2], bc = [b.x + b.w / 2, b.y + b.h / 2];
  const dx = bc[0] - ac[0], dy = bc[1] - ac[1];
  const horizontal = side ? side === "h" : Math.abs(dx) > Math.abs(dy);
  if (horizontal) {
    const s = dx > 0 ? [a.x + a.w, ac[1]] : [a.x, ac[1]];
    const t = dx > 0 ? [b.x - 2, bc[1]] : [b.x + b.w + 2, bc[1]];
    if (Math.abs(ac[1] - bc[1]) < 1) return [s, t];
    const mx = (s[0] + t[0]) / 2;
    return [s, [mx, s[1]], [mx, t[1]], t];
  }
  const s = dy > 0 ? [ac[0], a.y + a.h] : [ac[0], a.y];
  const t = dy > 0 ? [bc[0], b.y - 2] : [bc[0], b.y + b.h + 2];
  if (Math.abs(ac[0] - bc[0]) < 1) return [s, t];
  const my = (s[1] + t[1]) / 2;
  return [s, [s[0], my], [t[0], my], t];
}
function mid(pts) {
  // midpoint along the polyline by length
  let len = 0; const seg = [];
  for (let i = 1; i < pts.length; i++) { const l = Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]); seg.push(l); len += l; }
  let t = len / 2;
  for (let i = 0; i < seg.length; i++) {
    if (t <= seg[i]) { const r = seg[i] ? t / seg[i] : 0; return [pts[i][0] + (pts[i + 1][0] - pts[i][0]) * r, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * r]; }
    t -= seg[i];
  }
  return pts[pts.length - 1];
}

// Helpers for laying out rows/columns of nodes on a grid.
export const node = (id, x, y, w, h, label, extra = {}) => ({ id, x, y, w, h, label, ...extra });
