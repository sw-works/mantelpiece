// Renders the PWA icons without an image library: a dark rounded tile with an amber route
// and three photo dots, encoded straight to PNG. `node scripts/icons.mjs` rewrites
// public/icons/icon-{192,512}.png; the files are committed so a build needs nothing.
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";

const BG = [11, 11, 15];
const ROUTE = [255, 180, 84];
const DOT = [242, 242, 245];

function crc32(buf) {
  let c = ~0;
  for (const b of buf) {
    c ^= b;
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return ~c >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function png(size, pixel) {
  const raw = Buffer.alloc((size * 4 + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0;
    for (let x = 0; x < size; x++) {
      const [r, g, b, a] = pixel(x + 0.5, y + 0.5);
      raw.set([r, g, b, a], y * (size * 4 + 1) + 1 + x * 4);
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr.set([8, 6, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** Distance from a point to a segment. */
function segDist(px, py, ax, ay, bx, by) {
  const dx = bx - ax;
  const dy = by - ay;
  const t = Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(px - (ax + t * dx), py - (ay + t * dy));
}

/** Coverage in [0, 1] of a shape whose signed distance is `d` (negative inside). */
const cover = (d) => Math.max(0, Math.min(1, 0.5 - d));

function render(size) {
  const s = size / 512;
  const radius = 96 * s;
  // A route bending across the tile, in unit coordinates of the 512 design.
  const route = [
    [104, 392],
    [188, 300],
    [268, 332],
    [336, 212],
    [416, 128],
  ].map(([x, y]) => [x * s, y * s]);
  const dots = [route[0], route[2], route[4]];
  return (x, y) => {
    // Rounded square: signed distance.
    const qx = Math.abs(x - size / 2) - (size / 2 - radius);
    const qy = Math.abs(y - size / 2) - (size / 2 - radius);
    const tile = Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - radius;
    const alpha = cover(tile);
    if (alpha <= 0) return [0, 0, 0, 0];
    let color = BG;
    let road = Infinity;
    for (let i = 0; i + 1 < route.length; i++) road = Math.min(road, segDist(x, y, ...route[i], ...route[i + 1]));
    const roadCover = cover(road - 22 * s);
    if (roadCover > 0) color = mix(color, ROUTE, roadCover);
    for (const [dx, dy] of dots) {
      const ring = cover(Math.hypot(x - dx, y - dy) - 40 * s);
      if (ring > 0) color = mix(color, DOT, ring);
      const hole = cover(Math.hypot(x - dx, y - dy) - 18 * s);
      if (hole > 0) color = mix(color, BG, hole);
    }
    return [...color, Math.round(alpha * 255)];
  };
}

const mix = (a, b, t) => a.map((v, i) => Math.round(v + (b[i] - v) * t));

for (const size of [192, 512]) {
  writeFileSync(new URL(`../public/icons/icon-${size}.png`, import.meta.url), png(size, render(size)));
  console.log(`icon-${size}.png`);
}
