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
            { label: "Introduction", link: "/docs/" },
            { label: "Quickstart", link: "/docs/quickstart/" },
            { label: "Installation", link: "/docs/installation/" },
            { label: "Coming from another tool", link: "/docs/migrating/" },
          ],
        },
        {
          label: "Writing hooks",
          items: [
            { label: "Configuration", link: "/docs/configuration/" },
            { label: "Choosing the files", link: "/docs/files/" },
            { label: "Writing commands", link: "/docs/commands/" },
            { label: "Hooks graph and variables", link: "/docs/graph/" },
            { label: "Local changes", link: "/docs/local-changes/" },
          ],
        },
        {
          label: "Using hooks",
          items: [
            { label: "Running hooks", link: "/docs/running/" },
            { label: "Cache", link: "/docs/cache/" },
            { label: "Mono-repos", link: "/docs/monorepos/" },
            { label: "Continuous integration", link: "/docs/ci/" },
            { label: "Desktop app", link: "/docs/desktop-app/" },
          ],
        },
        {
          label: "Sharing hooks",
          items: [
            { label: "Shared hooks", link: "/docs/shared-hooks/" },
            { label: "Hooks of the pre-commit ecosystem", link: "/docs/precommit-hooks/" },
            { label: "Security", link: "/docs/security/" },
          ],
        },
        {
          label: "Reference",
          items: [
            { label: "Configuration reference", link: "/docs/config-reference/" },
            { label: "CLI reference", link: "/docs/cli/" },
            { label: "Environment and files", link: "/docs/environment/" },
            { label: "FAQ and troubleshooting", link: "/docs/faq/" },
          ],
        },
        {
          label: "Project",
          items: [
            { label: "Benchmarks", link: "https://github.com/zuygui/goat-hooker/tree/main/benchmarks" },
            { label: "Releasing", link: "/docs/releasing/" },
          ],
        },
      ],
    }),
  ],
});
