// Prints the gzip size of what the first paint loads (entry script, its static imports
// and stylesheets) and of everything in dist/. Run after `pnpm build`.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

const dist = new URL("../dist/", import.meta.url).pathname;
const html = readFileSync(join(dist, "index.html"), "utf8");
const gz = (p) => gzipSync(readFileSync(join(dist, p)), { level: 9 }).length;
const kb = (n) => `${(n / 1024).toFixed(1)} kB`;

const initial = new Set([...html.matchAll(/(?:src|href)="\/?(assets\/[^"]+\.(?:js|css))"/g)].map((m) => m[1]));
// Follow static imports of the entry chunk so preloaded dependencies count too.
for (const file of [...initial].filter((f) => f.endsWith(".js"))) {
  const code = readFileSync(join(dist, file), "utf8");
  for (const m of code.matchAll(/^import\s*(?:[^"']*from\s*)?["']\.\/([^"']+)["']/gm)) initial.add(`assets/${m[1]}`);
}

let first = 0;
for (const f of initial) { const n = gz(f); first += n; console.log(`initial  ${kb(n).padStart(9)}  ${f}`); }
const all = readdirSync(join(dist, "assets")).filter((f) => /\.(js|css)$/.test(f));
const total = all.reduce((s, f) => s + gz(`assets/${f}`), 0);
const fonts = readdirSync(join(dist, "assets")).filter((f) => f.endsWith(".woff2")).reduce((s, f) => s + statSync(join(dist, "assets", f)).size, 0);
console.log(`\nfirst paint (js+css, gzip): ${kb(first)}`);
console.log(`all js+css (gzip):          ${kb(total)} across ${all.length} files`);
console.log(`bundled fonts (woff2):      ${kb(fonts)}`);
