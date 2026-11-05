// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";
import remarkCjkFriendly from "remark-cjk-friendly";

// GitHub Pages project page: https://dotnetpower.github.io/agent-tokenomics/
// Override with SITE_URL / BASE_PATH to deploy elsewhere.
const SITE_URL = process.env.SITE_URL ?? "https://dotnetpower.github.io";
const IS_PROD = process.env.NODE_ENV === "production";
const BASE_PATH = process.env.BASE_PATH ?? (IS_PROD ? "/agent-tokenomics" : "/");

/** Open document content links (except same-page #anchors) in a new tab. */
function rehypeDocumentLinksNewTab() {
  const visit = (node) => {
    if (!node || typeof node !== "object") return;
    if (node.type === "element" && node.tagName === "a") {
      const props = (node.properties ??= {});
      const href = String(props.href ?? "").trim();
      if (href && !href.startsWith("#")) {
        props.target = "_blank";
        props.rel = "noopener noreferrer";
      }
    } else if (node.type === "raw" && typeof node.value === "string" && node.value.includes("<a ")) {
      node.value = node.value.replace(
        /<a\s+(?![^>]*\btarget=)([^>]*\bhref=["'](?!#)[^"']+["'][^>]*)>/gi,
        '<a target="_blank" rel="noopener noreferrer" $1>',
      );
    }
    if (Array.isArray(node.children)) {
      for (const child of node.children) visit(child);
    }
  };
  return (tree) => visit(tree);
}

export default defineConfig({
  site: SITE_URL,
  base: BASE_PATH,
  trailingSlash: "ignore",
  markdown: {
    remarkPlugins: [remarkCjkFriendly],
    rehypePlugins: [rehypeDocumentLinksNewTab],
  },
  integrations: [
    starlight({
      title: "agent-tokenomics",
      logo: {
        src: "./public/favicon.svg",
        replacesTitle: false,
      },
      favicon: "/favicon.svg",
      description:
        "Practical ways to save LLM tokens: VS Code GitHub Copilot, and a troubleshooting guide for LLM agents.",
      defaultLocale: "root",
      locales: {
        root: { label: "English", lang: "en" },
        ko: { label: "한국어", lang: "ko" },
      },
      customCss: ["./src/styles/custom.css"],
      components: {
        Head: "./src/components/Head.astro",
      },
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/dotnetpower/agent-tokenomics",
        },
      ],
      editLink: {
        baseUrl: "https://github.com/dotnetpower/agent-tokenomics/edit/main/site/",
      },
      sidebar: [
        {
          label: "Copilot Tokenomics",
          translations: { ko: "Copilot 토큰 절약" },
          autogenerate: { directory: "copilot-tokenomics" },
        },
        {
          label: "Agent Tokenomics",
          translations: { ko: "에이전트 토큰 절약" },
          autogenerate: { directory: "agent-tokenomics" },
        },
        {
          label: "Resources",
          translations: { ko: "참고 자료" },
          slug: "resources",
        },
        {
          label: "Papers",
          translations: { ko: "논문" },
          slug: "papers",
        },
        {
          label: "Glossary",
          translations: { ko: "용어집" },
          slug: "glossary",
        },
      ],
    }),
  ],
});
