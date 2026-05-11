import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { eventToBlocks } from "./stream";
import type { AgentEvent, DisplayBlock } from "./types";
import { Markdown } from "./components/Markdown";
import { ToolCallCard, ToolResultCard } from "./components/ToolCall";
import "./App.css";

function App() {
  const [blocks, setBlocks] = createSignal<DisplayBlock[]>([]);
  const [input, setInput] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  let unlisten: UnlistenFn | undefined;
  let logEl: HTMLDivElement | undefined;

  onMount(async () => {
    unlisten = await listen<AgentEvent>("claude://event", (e) => {
      const next = eventToBlocks(e.payload.raw);
      if (next.length === 0) return;
      setBlocks((prev) => [...prev, ...next]);
      queueMicrotask(() => logEl?.scrollTo({ top: logEl.scrollHeight }));
      // Mark not-busy when claude emits a `result` event (end of turn).
      if (e.payload.raw?.type === "result") setBusy(false);
    });
  });

  onCleanup(() => unlisten?.());

  async function send(e: Event) {
    e.preventDefault();
    const text = input().trim();
    if (!text || busy()) return;
    setInput("");
    setBlocks((prev) => [...prev, { kind: "text", role: "user", text }]);
    setBusy(true);
    try {
      await invoke("send_message", { text });
    } catch (err) {
      setBlocks((prev) => [...prev, { kind: "error", text: String(err) }]);
      setBusy(false);
    }
  }

  return (
    <main class="app">
      <header class="header">
        <h1>linux coworker</h1>
        <span class="hint">Claude Code · Linux desktop</span>
        <Show when={busy()}>
          <span class="spinner" aria-label="thinking" />
        </Show>
      </header>

      <div class="log" ref={logEl}>
        <Show when={blocks().length === 0}>
          <div class="empty">
            Ask the agent to do something on your machine. It can run commands,
            edit files, take a screenshot, click, and type.
          </div>
        </Show>
        <For each={blocks()}>{(b) => <BlockView block={b} />}</For>
      </div>

      <form class="composer" onSubmit={send}>
        <textarea
          value={input()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              send(e);
            }
          }}
          placeholder="Tell the agent what to do…  (Shift+Enter for newline)"
          rows={3}
        />
        <button type="submit" disabled={busy()}>
          {busy() ? "…" : "Send"}
        </button>
      </form>
    </main>
  );
}

function BlockView(props: { block: DisplayBlock }) {
  const b = () => props.block;
  return (
    <>
      <Show when={b().kind === "text"}>
        {(() => {
          const t = b() as Extract<DisplayBlock, { kind: "text" }>;
          return (
            <div class={`msg ${t.role}`}>
              <div class="role">{t.role}</div>
              <div class="text">
                <Show when={t.role === "assistant"} fallback={<div class="user-text">{t.text}</div>}>
                  <Markdown source={t.text} />
                </Show>
              </div>
            </div>
          );
        })()}
      </Show>
      <Show when={b().kind === "tool_call"}>
        <ToolCallCard call={(b() as any).call} />
      </Show>
      <Show when={b().kind === "tool_result"}>
        <ToolResultCard result={(b() as any).result} />
      </Show>
      <Show when={b().kind === "system"}>
        <div class="system">{(b() as any).text}</div>
      </Show>
      <Show when={b().kind === "error"}>
        <div class="error">{(b() as any).text}</div>
      </Show>
    </>
  );
}

export default App;
