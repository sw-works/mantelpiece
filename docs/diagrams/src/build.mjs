// Renders every diagram module in this directory to ../<name>.svg and ../<name>.png.
//
//   node src/build.mjs           # all
//   node src/build.mjs 05 07     # only modules whose filename contains "05" or "07"
//
// A module is any `NN-name.mjs` or `pN-name.mjs` that default-exports a function
// returning an SVG string. PNG rendering needs playwright + chromium; without it
// the script writes SVGs only and says so.
import { execFileSync } from "node:child_process";
import { readdirSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, "..");
const only = process.argv.slice(2);
const files = readdirSync(here)
  .filter((f) => /^(\d\d|p\d+)-.*\.mjs$/.test(f))
  .filter((f) => !only.length || only.some((o) => f.includes(o)))
  .sort();

if (!files.length) {
  console.error(only.length ? `no diagram modules match: ${only.join(" ")}` : "no diagram modules found");
  process.exit(1);
}

// Playwright may live beside this script, in the project, in a parent, or in an
// explicitly named install (PLAYWRIGHT_DIR points at the directory whose
// node_modules holds it). The skill directory keeps its own copy, so a project
// that never installed playwright still renders PNGs.
async function loadChromium() {
  const skill = join(process.env.HOME ?? "", ".claude/skills/diagram/assets");
  const roots = [process.env.PLAYWRIGHT_DIR, here, out, join(out, ".."), process.cwd(), skill].filter(Boolean);
  for (const root of roots) {
    try {
      return createRequire(join(root, "package.json"))("playwright").chromium;
    } catch {
      // try the next root
    }
  }
  // Last resort: a global install. Node never searches the global root itself,
  // so ask npm where it is (~200ms, and only when nothing local matched).
  try {
    const global = execFileSync("npm", ["root", "-g"], { encoding: "utf8" }).trim();
    return createRequire(join(global, "package.json"))("playwright").chromium;
  } catch {
    return null;
  }
}

const chromium = await loadChromium();
if (!chromium) console.error("playwright not found — writing SVG only (npm i -D playwright && npx playwright install chromium)");

const browser = chromium ? await chromium.launch() : null;
for (const f of files) {
  const name = f.replace(/\.mjs$/, "");
  const mod = await import(pathToFileURL(join(here, f)).href);
  if (typeof mod.default !== "function") throw new Error(`${f} must default-export a function returning SVG`);
  const svg = mod.default();
  writeFileSync(join(out, `${name}.svg`), svg);
  if (browser) {
    const page = await browser.newPage({ deviceScaleFactor: 2 });
    await page.setContent(`<!doctype html><body style="margin:0;background:#fff">${svg}</body>`);
    const el = await page.$("svg");
    await el.screenshot({ path: join(out, `${name}.png`), omitBackground: false });
    await page.close();
  }
  console.error(`✓ ${name}.svg${browser ? " + .png" : ""}`);
}
if (browser) await browser.close();
