import { createEffect, createMemo, createResource, createSignal, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import hljs from "highlight.js/lib/core";
import { Markdown } from "./Markdown";

type Mode = "code" | "preview";

export function FilePreview(props: {
  path: string;
  onClose: () => void;
  refreshKey: number; // bump to force re-fetch (e.g. when another Edit lands)
}) {
  const isMd = () => /\.(md|markdown)$/i.test(props.path);
  const [mode, setMode] = createSignal<Mode>(isMd() ? "preview" : "code");

  // (Re)load whenever path or refreshKey changes.
  const [data, { refetch }] = createResource(
    () => [props.path, props.refreshKey] as const,
    async ([path]) => {
      try {
        return await invoke<string>("read_file", { path });
      } catch (e) {
        throw new Error(String(e));
      }
    },
  );

  // When user switches files, reset mode default appropriately.
  createEffect(() => {
    setMode(isMd() ? "preview" : "code");
  });

  const lang = createMemo(() => detectLang(props.path));

  const indentUnit = createMemo(() => detectIndentUnit(data() ?? ""));

  const codeLines = createMemo(() => {
    const txt = data();
    if (txt == null) return [] as { html: string; indent: number }[];
    const l = lang();
    let html: string;
    if (l && hljs.getLanguage(l)) {
      try {
        html = hljs.highlight(txt, { language: l, ignoreIllegals: true }).value;
      } catch {
        html = escapeHtml(txt);
      }
    } else {
      try {
        html = hljs.highlightAuto(txt).value;
      } catch {
        html = escapeHtml(txt);
      }
    }
    const htmlLines = splitHighlightedLines(html);
    const rawLines = txt.split("\n");
    const unit = indentUnit();
    return htmlLines.map((h, i) => ({
      html: h,
      indent: countLeadingCols(rawLines[i] ?? "", unit),
    }));
  });

  function basename(p: string): string {
    const i = p.lastIndexOf("/");
    return i >= 0 ? p.slice(i + 1) : p;
  }

  return (
    <aside class="preview-pane">
      <header class="preview-head">
        <div class="preview-name" title={props.path}>
          <span class="preview-basename">{basename(props.path)}</span>
          <span class="preview-dir">{props.path}</span>
        </div>
        <div class="preview-actions">
          <Show when={isMd()}>
            <button
              type="button"
              class="preview-toggle"
              onClick={() => setMode((m) => (m === "code" ? "preview" : "code"))}
              title="Toggle code / rendered markdown"
            >
              {mode() === "code" ? "Preview" : "Code"}
            </button>
          </Show>
          <button
            type="button"
            class="preview-refresh"
            onClick={() => refetch()}
            title="Reload from disk"
          >
            ↻
          </button>
          <button
            type="button"
            class="preview-close"
            onClick={props.onClose}
            title="Close preview"
          >
            ×
          </button>
        </div>
      </header>
      <div class="preview-body">
        <Show
          when={data.error}
          fallback={
            <Show when={data()} fallback={<div class="preview-loading">loading…</div>}>
              <Show
                when={isMd() && mode() === "preview"}
                fallback={
                  <pre
                    class="preview-code hljs"
                    style={{ "--indent-unit": `${indentUnit()}ch` }}
                  >
                    {codeLines().map((row) => (
                      <div
                        class="preview-code-line"
                        style={{ "--indent-cols": String(row.indent) }}
                      >
                        <span
                          class="preview-code-content"
                          innerHTML={row.html || "​"}
                        />
                      </div>
                    ))}
                  </pre>
                }
              >
                <div class="preview-md">
                  <Markdown source={data() ?? ""} />
                </div>
              </Show>
            </Show>
          }
        >
          <div class="preview-error">{String(data.error)}</div>
        </Show>
      </div>
    </aside>
  );
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function detectLang(path: string): string | undefined {
  const ext = path.split(".").pop()?.toLowerCase();
  if (!ext) return undefined;
  const map: Record<string, string> = {
    ts: "typescript",
    tsx: "typescript",
    js: "javascript",
    jsx: "javascript",
    mjs: "javascript",
    cjs: "javascript",
    py: "python",
    rs: "rust",
    go: "go",
    json: "json",
    yml: "yaml",
    yaml: "yaml",
    html: "xml",
    htm: "xml",
    xml: "xml",
    css: "css",
    scss: "css",
    sh: "bash",
    bash: "bash",
    zsh: "bash",
    md: "markdown",
    markdown: "markdown",
    sql: "sql",
    diff: "diff",
    patch: "diff",
    toml: "ini",
    ini: "ini",
  };
  return map[ext];
}

/// Split highlight.js HTML output into one entry per source line, carrying
/// open `<span>` tags across line breaks so each line stays valid HTML on
/// its own. Required because hljs spans can wrap multi-line constructs
/// (block comments, template literals, etc.).
function splitHighlightedLines(html: string): string[] {
  const lines: string[] = [];
  const stack: string[] = [];
  let cur = "";
  let i = 0;
  while (i < html.length) {
    const ch = html[i];
    if (ch === "<") {
      const end = html.indexOf(">", i);
      if (end === -1) {
        cur += html.slice(i);
        break;
      }
      const tag = html.slice(i, end + 1);
      if (tag.startsWith("</")) {
        stack.pop();
      } else if (tag.startsWith("<span")) {
        stack.push(tag);
      }
      cur += tag;
      i = end + 1;
    } else if (ch === "\n") {
      cur += "</span>".repeat(stack.length);
      lines.push(cur);
      cur = stack.join("");
      i++;
    } else {
      cur += ch;
      i++;
    }
  }
  // Final line (don't drop it even if empty so file ends with one blank row).
  if (cur || lines.length === 0) {
    cur += "</span>".repeat(stack.length);
    lines.push(cur);
  }
  return lines;
}

/// Count leading whitespace columns on a raw source line, expanding tabs to
/// the configured indent unit. Returns the column at which non-whitespace
/// content begins (rounded down to a multiple of `unit` so the indent guide
/// stops at the deepest fully-occupied indent level).
function countLeadingCols(line: string, unit: number): number {
  let cols = 0;
  for (const ch of line) {
    if (ch === " ") cols += 1;
    else if (ch === "\t") cols += unit;
    else break;
  }
  // Snap down to the nearest indent unit so partial indentation doesn't
  // draw a guide past the real depth.
  return Math.floor(cols / unit) * unit;
}

/// Detect the indent unit (2 or 4 spaces, or a tab). Falls back to 2.
function detectIndentUnit(text: string): number {
  let twos = 0;
  let fours = 0;
  for (const line of text.split("\n", 200)) {
    const m = line.match(/^( +)\S/);
    if (!m) continue;
    const w = m[1].length;
    if (w === 4 || w === 8) fours++;
    else if (w === 2 || w === 6) twos++;
  }
  if (fours > twos) return 4;
  return 2;
}
