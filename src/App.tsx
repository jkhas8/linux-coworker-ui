import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { eventToBlocks } from "./stream";
import type { AgentEvent, Attachment, DisplayBlock } from "./types";
import { Markdown } from "./components/Markdown";
import { ToolCallCard, ToolResultCard } from "./components/ToolCall";
import { ThinkingBlock } from "./components/Thinking";
import { AskQuestionForm } from "./components/AskQuestion";
import { AttachmentStrip } from "./components/Attachments";
import { fileToAttachment, imagesFromDataTransfer } from "./attachments";
import "./App.css";

function App() {
  const [blocks, setBlocks] = createSignal<DisplayBlock[]>([]);
  const [input, setInput] = createSignal("");
  const [attachments, setAttachments] = createSignal<Attachment[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [dragOver, setDragOver] = createSignal(false);

  let unlisten: UnlistenFn | undefined;
  let logEl: HTMLDivElement | undefined;
  let fileInputEl: HTMLInputElement | undefined;

  onMount(async () => {
    unlisten = await listen<AgentEvent>("claude://event", (e) => {
      const next = eventToBlocks(e.payload.raw);
      if (next.length === 0) return;
      setBlocks((prev) => {
        // Suppress the error tool_result that follows an AskUserQuestion —
        // the model can't actually run that tool in --print mode, so the
        // result is always an error frame. Our form replaces the tool.
        const askIds = new Set(
          prev
            .filter((b): b is Extract<DisplayBlock, { kind: "ask_question" }> => b.kind === "ask_question")
            .map((b) => b.id),
        );
        const filtered = next.filter((b) => {
          if (b.kind === "tool_result" && askIds.has(b.result.tool_use_id)) return false;
          return true;
        });
        return [...prev, ...filtered];
      });
      queueMicrotask(() => logEl?.scrollTo({ top: logEl.scrollHeight }));
      if (e.payload.raw?.type === "result") setBusy(false);
    });
  });

  async function submitAskAnswers(id: string, lines: string[]) {
    // Mark the question block as submitted so the form locks.
    setBlocks((prev) =>
      prev.map((b) =>
        b.kind === "ask_question" && b.id === id
          ? { ...b, submitted: { lines } }
          : b,
      ),
    );
    // Send the answers as a normal user reply — the model will pick them up
    // on the next turn via --resume.
    const text = lines.join("\n");
    setBusy(true);
    try {
      await invoke("send_message", { text });
    } catch (err) {
      setBlocks((prev) => [...prev, { kind: "error", text: String(err) }]);
      setBusy(false);
    }
  }

  onCleanup(() => unlisten?.());

  async function addFiles(files: (File | Blob)[]) {
    for (const f of files) {
      const r = await fileToAttachment(f);
      if ("reason" in r) {
        setBlocks((prev) => [...prev, { kind: "error", text: r.reason }]);
      } else {
        setAttachments((prev) => [...prev, r]);
      }
    }
  }

  function removeAttachment(id: string) {
    setAttachments((prev) => prev.filter((a) => a.id !== id));
  }

  async function onPaste(e: ClipboardEvent) {
    // Fast path: image bytes live on the synchronous clipboardData.
    const direct = imagesFromDataTransfer(e.clipboardData);
    if (direct.length > 0) {
      e.preventDefault();
      await addFiles(direct);
      return;
    }
    // Slow path: webkit2gtk often omits images from clipboardData on Linux.
    // Fall back to the async Clipboard API and additively attach anything we find.
    if (!navigator.clipboard?.read) return;
    try {
      const items = await navigator.clipboard.read();
      const blobs: Blob[] = [];
      for (const it of items) {
        for (const t of it.types) {
          if (t.startsWith("image/")) blobs.push(await it.getType(t));
        }
      }
      if (blobs.length > 0) await addFiles(blobs);
    } catch {
      // permission denied or nothing useful — ignore quietly
    }
  }

  function newConversation() {
    // Clear the UI immediately so the click feels responsive; reap the
    // claude subprocess in the background.
    setBlocks([]);
    setInput("");
    setAttachments([]);
    setBusy(false);
    invoke("end_session").catch(() => {
      /* session may not have been started — fine */
    });
  }

  async function onDrop(e: DragEvent) {
    e.preventDefault();
    setDragOver(false);
    const imgs = imagesFromDataTransfer(e.dataTransfer);
    if (imgs.length === 0) return;
    await addFiles(imgs);
  }

  async function onPickFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    if (!input.files) return;
    await addFiles(Array.from(input.files));
    input.value = "";
  }

  async function send(e: Event) {
    e.preventDefault();
    const text = input().trim();
    const atts = attachments();
    if ((!text && atts.length === 0) || busy()) return;

    // Render the user's message locally before clearing.
    setBlocks((prev) => {
      const next: DisplayBlock[] = [...prev];
      for (const a of atts) {
        next.push({
          kind: "image",
          role: "user",
          mimeType: a.mimeType,
          data: a.data,
          alt: a.name,
        });
      }
      if (text) next.push({ kind: "text", role: "user", text });
      return next;
    });
    setInput("");
    setAttachments([]);
    setBusy(true);
    try {
      await invoke("send_message", {
        text,
        attachments: atts.map((a) => ({ mediaType: a.mimeType, data: a.data })),
      });
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
        <button
          type="button"
          class="new-chat"
          title="Start a new conversation"
          onClick={newConversation}
          disabled={blocks().length === 0 && !busy()}
        >
          + New
        </button>
      </header>

      <div class="log" ref={logEl}>
        <Show when={blocks().length === 0}>
          <div class="empty">
            Ask the agent to do something on your machine. It can run commands,
            edit files, take a screenshot, click, and type.
            <br />
            <span class="empty-hint">
              Paste an image, drop a file, or click 📎 to attach.
            </span>
          </div>
        </Show>
        <For each={blocks()}>
          {(b) => <BlockView block={b} onAskSubmit={submitAskAnswers} />}
        </For>
      </div>

      <form
        class="composer"
        classList={{ "drag-over": dragOver() }}
        onSubmit={send}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={onDrop}
      >
        <AttachmentStrip items={attachments()} onRemove={removeAttachment} />
        <div class="composer-row">
          <button
            type="button"
            class="attach-btn"
            title="Attach image"
            onClick={() => fileInputEl?.click()}
          >
            📎
          </button>
          <input
            ref={fileInputEl}
            type="file"
            accept="image/png,image/jpeg,image/gif,image/webp"
            multiple
            hidden
            onChange={onPickFile}
          />
          <textarea
            value={input()}
            onInput={(e) => setInput(e.currentTarget.value)}
            onPaste={onPaste}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send(e);
              }
            }}
            placeholder="Tell the agent what to do…  (Shift+Enter for newline · paste/drop images)"
            rows={3}
          />
          <button type="submit" disabled={busy()}>
            {busy() ? "…" : "Send"}
          </button>
        </div>
      </form>
    </main>
  );
}

function BlockView(props: {
  block: DisplayBlock;
  onAskSubmit: (id: string, lines: string[]) => void;
}) {
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
                  <div class="answer">
                    <Markdown source={t.text} />
                  </div>
                </Show>
              </div>
            </div>
          );
        })()}
      </Show>
      <Show when={b().kind === "thinking"}>
        {(() => {
          const t = b() as Extract<DisplayBlock, { kind: "thinking" }>;
          return <ThinkingBlock text={t.text} redacted={t.redacted} />;
        })()}
      </Show>
      <Show when={b().kind === "image"}>
        {(() => {
          const i = b() as Extract<DisplayBlock, { kind: "image" }>;
          return (
            <div class={`msg ${i.role}`}>
              <div class="role">{i.role}</div>
              <div class="text">
                <img
                  class="msg-image"
                  src={`data:${i.mimeType};base64,${i.data}`}
                  alt={i.alt ?? "image"}
                />
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
      <Show when={b().kind === "ask_question"}>
        {(() => {
          const q = b() as Extract<DisplayBlock, { kind: "ask_question" }>;
          return (
            <AskQuestionForm
              questions={q.questions}
              submitted={q.submitted}
              onSubmit={(lines) => props.onAskSubmit(q.id, lines)}
            />
          );
        })()}
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
