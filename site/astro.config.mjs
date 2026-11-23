// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

export default defineConfig({
  site: "https://zuygui.github.io/goat-hooker/",
  base: "/goat-hooker/",
  integrations: [
    starlight({
      title: "Goat Hooker",
      description: "Git hooks that only run what changed, for every project of your mono-repo.",
      logo: { src: "./src/assets/goat.png", alt: "Goat Hooker" },
      favicon: "/favicon-32.png",
      head: [
        { tag: "link", attrs: { rel: "apple-touch-icon", href: "/goat-hooker/apple-touch-icon.png" } },
        { tag: "link", attrs: { rel: "preconnect", href: "https://fonts.googleapis.com" } },
        { tag: "link", attrs: { rel: "preconnect", href: "https://fonts.gstatic.com", crossorigin: "" } },
        {
          tag: "link",
          attrs: {
            rel: "stylesheet",
            href: "https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700;800;900&family=JetBrains+Mono:wght@400;500;700&display=swap",
          },
        },
      ],
      customCss: ["./src/styles/docs.css"],
      social: [{ icon: "github", label: "GitHub", href: "https://github.com/zuygui/goat-hooker" }],
      sidebar: [
        {
          label: "Getting started",
          items: [
            { label: "Quickstart", link: "/docs/quickstart/" },
            { label: "Installation", link: "/docs/installation/" },
          ],
        },
        {
          label: "Guides",
          items: [
            { label: "Configuration", link: "/docs/configuration/" },
            { label: "Mono-repos", link: "/docs/monorepos/" },
            { label: "Hooks graph and variables", link: "/docs/graph/" },
            { label: "Shared hooks", link: "/docs/shared-hooks/" },
            { label: "Desktop app", link: "/docs/desktop-app/" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "The hooker CLI", link: "/docs/cli/" },
            { label: "Releasing", link: "/docs/releasing/" },
          ],
        },
      ],
    }),
  ],
});
