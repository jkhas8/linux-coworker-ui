import { createEffect, createMemo, onCleanup } from "solid-js";
import mermaid from "mermaid";
import { renderMarkdown } from "../markdown";

let mermaidInitialized = false;
function ensureMermaid() {
  if (mermaidInitialized) return;
  mermaidInitialized = true;
  mermaid.initialize({
    startOnLoad: false,
    theme: "dark",
    securityLevel: "loose",
    fontFamily:
      "ui-monospace, SFMono-Regular, 'JetBrains Mono', Menlo, monospace",
  });
}

let diagramSeq = 0;

export function Markdown(props: { source: string }) {
  ensureMermaid();
  let container: HTMLDivElement | undefined;
  const html = createMemo(() => renderMarkdown(props.source));

  let cancelled = false;
  onCleanup(() => {
    cancelled = true;
  });

  createEffect(() => {
    // Track html() so the effect re-runs when content changes.
    html();
    if (!container) return;

    const nodes = container.querySelectorAll<HTMLElement>(
      ".mermaid-block[data-mermaid='1']",
    );
    if (nodes.length === 0) return;

    nodes.forEach(async (node) => {
      const source = (node.textContent ?? "").trim();
      if (!source) return;
      const id = `mermaid-${diagramSeq++}`;
      // Mark immediately so a re-render of the same content doesn't double-render.
      node.setAttribute("data-mermaid", "rendering");
      try {
        const { svg } = await mermaid.render(id, source);
        if (cancelled) return;
        node.innerHTML = svg;
        node.setAttribute("data-mermaid", "done");
      } catch (e) {
        if (cancelled) return;
        node.innerHTML = `<pre class="mermaid-error">${escapeHtml(
          (e as Error)?.message ?? String(e),
        )}\n\n${escapeHtml(source)}</pre>`;
        node.setAttribute("data-mermaid", "error");
      }
    });
  });

  return <div ref={container} class="md" innerHTML={html()} />;
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
