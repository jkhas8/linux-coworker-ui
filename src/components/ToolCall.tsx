import { createSignal, Match, Show, Switch } from "solid-js";
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

  // Detect image-bearing content first — render inline.
  const imageBlock = () => {
    const c = content();
    if (!Array.isArray(c)) return null;
    return c.find((x: any) => x?.type === "image") ?? null;
  };

  const textBlock = () => {
    const c = content();
    if (typeof c === "string") return c;
    if (Array.isArray(c)) {
      return c
        .filter((x: any) => x?.type === "text")
        .map((x: any) => x.text)
        .join("\n");
    }
    return "";
  };

  return (
    <div class="tool-result" classList={{ err: !!r().is_error }}>
      <Show when={imageBlock()}>
        {(img) => (
          <img
            class="screenshot"
            src={`data:${(img() as any).mimeType ?? "image/png"};base64,${(img() as any).data}`}
            alt="screenshot"
          />
        )}
      </Show>
      <Show when={textBlock()}>
        {(t) => (
          <Show
            when={r().is_error}
            fallback={
              <Show
                when={looksLikeMarkdown(t())}
                fallback={<pre class="result-pre">{t()}</pre>}
              >
                <Markdown source={t()} />
              </Show>
            }
          >
            <pre class="result-pre err">{t()}</pre>
          </Show>
        )}
      </Show>
    </div>
  );
}

function looksLikeMarkdown(s: string): boolean {
  // Heuristic: avoid markdown for short single-line outputs (e.g. command results).
  if (s.length < 60 && !s.includes("\n")) return false;
  return /[`*_#>\[]|```/.test(s);
}
