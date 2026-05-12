import { createMemo, createSignal, Match, onCleanup, Show, Switch } from "solid-js";
import type { ToolCall, ToolResult } from "../types";
import { Markdown } from "./Markdown";

const NICE_NAME: Record<string, string> = {
  Bash: "Run command",
  Read: "Read file",
  Write: "Write file",
  Edit: "Edit file",
  Glob: "Find files",
  Grep: "Search",
  TodoWrite: "Update todos",
  WebFetch: "Fetch URL",
  WebSearch: "Web search",
};

function prettyName(name: string): string {
  // Strip MCP prefix: mcp__linux_control__screenshot -> screenshot
  const stripped = name.replace(/^mcp__[^_]+__/, "");
  return NICE_NAME[stripped] ?? stripped;
}

function isMcp(name: string): boolean {
  return name.startsWith("mcp__");
}

export function ToolCallCard(props: { call: ToolCall }) {
  const [open, setOpen] = createSignal(false);
  const name = () => props.call.name;
  const input = () => props.call.input ?? {};

  return (
    <div class="tool" classList={{ mcp: isMcp(name()) }}>
      <button class="tool-head" onClick={() => setOpen(!open())}>
        <span class="dot" />
        <span class="tool-name">{prettyName(name())}</span>
        <Switch>
          <Match when={name() === "Bash"}>
            <code class="tool-summary">{(input() as any).command}</code>
          </Match>
          <Match when={name() === "Read" || name() === "Write" || name() === "Edit"}>
            <code class="tool-summary">{(input() as any).file_path}</code>
          </Match>
          <Match when={name() === "Grep"}>
            <code class="tool-summary">
              {(input() as any).pattern}
              <Show when={(input() as any).path}> · {(input() as any).path}</Show>
            </code>
          </Match>
          <Match when={name() === "Glob"}>
            <code class="tool-summary">{(input() as any).pattern}</code>
          </Match>
          <Match when={true}>
            <span class="tool-summary muted">{Object.keys(input()).slice(0, 3).join(", ")}</span>
          </Match>
        </Switch>
        <span class="chev">{open() ? "▾" : "▸"}</span>
      </button>
      <Show when={open()}>
        <pre class="tool-body">{JSON.stringify(input(), null, 2)}</pre>
      </Show>
    </div>
  );
}

export function ToolResultCard(props: { result: ToolResult }) {
  const r = () => props.result;
  const content = () => r().content;
  const [open, setOpen] = createSignal(false);

  // Detect image-bearing content first — render inline. Tool result image
  // blocks can come in two shapes:
  //   MCP raw:        { type: "image", data, mimeType }
  //   Anthropic API:  { type: "image", source: { type: "base64", data, media_type } }
  // claude code typically re-wraps MCP images into the API shape before
  // emitting them on the stream, so we accept both.
  const imageBlock = (): { data: string; mime: string } | null => {
    const c = content();
    if (!Array.isArray(c)) return null;
    for (const x of c as any[]) {
      if (!x || x.type !== "image") continue;
      if (typeof x.data === "string") {
        return { data: x.data, mime: x.mimeType ?? "image/png" };
      }
      if (x.source && typeof x.source.data === "string") {
        return {
          data: x.source.data,
          mime: x.source.media_type ?? x.source.mimeType ?? "image/png",
        };
      }
    }
    return null;
  };

  const textBlock = () => {
    const c = content();
    if (typeof c === "string") return c;
    if (Array.isArray(c)) {
      return c
        .map((x: any) => {
          if (x?.type === "text") return x.text;
          if (x?.type === "web_search_result") {
            const title = x.title ?? "(no title)";
            const url = x.url ?? "";
            return `- [${title}](${url})`;
          }
          return null;
        })
        .filter((s): s is string => typeof s === "string")
        .join("\n");
    }
    return "";
  };

  // Convert the base64 image payload into a Blob object URL. data: URLs work
  // for small images but webkit2gtk silently drops very large screenshots
  // (full-screen 1080p+ → multi-megabyte base64). Object URLs have no length
  // cap. The URL is revoked when the block unmounts.
  const imageUrl = createMemo<string | null>(() => {
    const img = imageBlock();
    if (!img) return null;
    try {
      const bytes = base64ToBytes(img.data);
      const blob = new Blob([bytes], { type: img.mime });
      return URL.createObjectURL(blob);
    } catch (e) {
      console.error("[ToolResultCard] failed to decode image", e, {
        bytesLen: img.data?.length,
      });
      return null;
    }
  });
  onCleanup(() => {
    const u = imageUrl();
    if (u) URL.revokeObjectURL(u);
  });

  const previewText = () => {
    const t = textBlock();
    if (!t) return "";
    const firstLine = t.split("\n").find((l) => l.trim()) ?? "";
    return firstLine.length > 100 ? firstLine.slice(0, 100) + "…" : firstLine;
  };
  const lineCount = () =>
    textBlock()
      .split(/\r?\n/)
      .filter((l) => l.trim()).length;

  return (
    <div class="tool-result" classList={{ err: !!r().is_error, open: open() }}>
      <Show when={imageUrl()}>
        {(url) => <img class="screenshot" src={url()} alt="screenshot" />}
      </Show>
      <Show when={textBlock()}>
        <button
          type="button"
          class="tool-result-head"
          onClick={() => setOpen((o) => !o)}
        >
          <span class="chev">›</span>
          <span class="tool-result-label">
            {r().is_error ? "error" : "tool result"}
          </span>
          <span class="tool-result-preview">{previewText()}</span>
          <span class="tool-result-meta">
            {lineCount()} line{lineCount() === 1 ? "" : "s"}
          </span>
        </button>
        <Show when={open()}>
          {(() => {
            const t = textBlock();
            if (r().is_error) {
              return <pre class="result-pre err">{t}</pre>;
            }
            if (looksLikeMarkdown(t)) {
              return (
                <div class="tool-result-body">
                  <Markdown source={t} />
                </div>
              );
            }
            return <pre class="result-pre">{t}</pre>;
          })()}
        </Show>
      </Show>
    </div>
  );
}

function base64ToBytes(b64: string): Uint8Array {
  const clean = b64.replace(/\s+/g, "");
  const bin = atob(clean);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function looksLikeMarkdown(s: string): boolean {
  // Heuristic: avoid markdown for short single-line outputs (e.g. command results).
  if (s.length < 60 && !s.includes("\n")) return false;
  return /[`*_#>\[]|```/.test(s);
}
