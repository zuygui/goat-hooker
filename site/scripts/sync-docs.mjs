// Copies the documentation of the repository (docs/*.md) into the Starlight content
// collection, so that docs/ stays the only place the documentation is written.
//
// - the first `# Title` becomes the `title` of the page
// - links between pages (`cli.md#cache`) point to the pages of the site
// - links to the rest of the repository point to GitHub

import { copyFileSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const BASE = "/goat-hooker/docs/";
const REPOSITORY = "https://github.com/zuygui/goat-hooker/blob/main/";
const source = new URL("../../docs/", import.meta.url).pathname;
const target = new URL("../src/content/docs/docs/", import.meta.url).pathname;

rmSync(target, { recursive: true, force: true });
mkdirSync(target, { recursive: true });

for (const file of readdirSync(source).filter((f) => f.endsWith(".md"))) {
  let content = readFileSync(join(source, file), "utf8");
  const heading = content.match(/^# (.+)$/m);
  const title = heading ? heading[1].replace(/`/g, "") : file.replace(/\.md$/, "");
  if (heading) content = content.replace(heading[0] + "\n", "");

  content = content.replace(/\]\(([^)\s]+)\)/g, (match, link) => {
    if (/^(https?:|mailto:|#)/.test(link)) return match;
    const [path, anchor] = link.split("#");
    const hash = anchor ? `#${anchor}` : "";
    if (!path.includes("/") && path.endsWith(".md")) {
      const slug = path === "README.md" ? "" : path.replace(/\.md$/, "/");
      return `](${BASE}${slug}${hash})`;
    }
    // Elsewhere in the repository
    const relative = path.replace(/^\.\.\//, "");
    const kind = relative.endsWith("/") || !relative.includes(".") ? "tree" : "blob";
    return `](${REPOSITORY.replace("/blob/", `/${kind}/`)}${relative}${hash})`;
  });

  const name = file === "README.md" ? "index.md" : file;
  const frontmatter = `---\ntitle: ${JSON.stringify(title)}\neditUrl: ${REPOSITORY.replace("/blob/", "/edit/")}docs/${file}\n---\n\n`;
  writeFileSync(join(target, name), frontmatter + content.trimStart());
}
// Images and data of the repository used by the landing page
const root = new URL("../../", import.meta.url).pathname;
const site = new URL("../", import.meta.url).pathname;
mkdirSync(join(site, "public"), { recursive: true });
mkdirSync(join(site, "src/data"), { recursive: true });
const copies = {
  "gui/assets/images/goat.png": "public/goat.png",
  "gui/packaging/32x32.png": "public/favicon-32.png",
  "gui/packaging/128x128@2x.png": "public/apple-touch-icon.png",
  ".github/assets/app-dark.png": "public/app-dark.png",
  ".github/assets/graph-dark.png": "public/graph-dark.png",
  "benchmarks/results.json": "src/data/benchmarks.json",
};
for (const [from, to] of Object.entries(copies)) copyFileSync(join(root, from), join(site, to));

console.log("docs synced");
