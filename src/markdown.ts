// Render markdown to sanitized HTML with syntax-highlighted code blocks.

import { marked } from "marked";
import DOMPurify from "dompurify";
import hljs from "highlight.js/lib/core";
import bash from "highlight.js/lib/languages/bash";
import css from "highlight.js/lib/languages/css";
import diff from "highlight.js/lib/languages/diff";
import go from "highlight.js/lib/languages/go";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import python from "highlight.js/lib/languages/python";
import rust from "highlight.js/lib/languages/rust";
import shell from "highlight.js/lib/languages/shell";
import sql from "highlight.js/lib/languages/sql";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

hljs.registerLanguage("bash", bash);
hljs.registerLanguage("sh", shell);
hljs.registerLanguage("shell", shell);
hljs.registerLanguage("css", css);
hljs.registerLanguage("diff", diff);
hljs.registerLanguage("go", go);
hljs.registerLanguage("json", json);
hljs.registerLanguage("md", markdown);
hljs.registerLanguage("markdown", markdown);
hljs.registerLanguage("py", python);
hljs.registerLanguage("python", python);
hljs.registerLanguage("rs", rust);
hljs.registerLanguage("rust", rust);
hljs.registerLanguage("sql", sql);
hljs.registerLanguage("ts", typescript);
hljs.registerLanguage("tsx", typescript);
hljs.registerLanguage("typescript", typescript);
hljs.registerLanguage("js", typescript);
hljs.registerLanguage("jsx", typescript);
hljs.registerLanguage("javascript", typescript);
hljs.registerLanguage("html", xml);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("yaml", yaml);
hljs.registerLanguage("yml", yaml);

marked.setOptions({ gfm: true, breaks: true });

marked.use({
  renderer: {
    code({ text, lang }: { text: string; lang?: string }) {
      let highlighted: string;
      let langClass = "";
      if (lang && hljs.getLanguage(lang)) {
        try {
          highlighted = hljs.highlight(text, { language: lang, ignoreIllegals: true }).value;
          langClass = ` language-${lang}`;
        } catch {
          highlighted = escapeHtml(text);
        }
      } else {
        try {
          const auto = hljs.highlightAuto(text);
          highlighted = auto.value;
          if (auto.language) langClass = ` language-${auto.language}`;
        } catch {
          highlighted = escapeHtml(text);
        }
      }
      return `<pre><code class="hljs${langClass}">${highlighted}</code></pre>`;
    },
  },
});

// Open all links in a new tab and harden rel.
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "A") {
    node.setAttribute("target", "_blank");
    node.setAttribute("rel", "noopener noreferrer");
  }
});

export function renderMarkdown(src: string): string {
  const raw = marked.parse(src, { async: false }) as string;
  return DOMPurify.sanitize(raw, { ADD_ATTR: ["target", "rel"] });
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
