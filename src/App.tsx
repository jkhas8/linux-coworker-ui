import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { eventToBlocks } from "./stream";
import type {
  AgentEvent,
  Attachment,
  ConversationSummary,
  DisplayBlock,
  Workspace,
} from "./types";
import { ToolCallCard, ToolResultCard } from "./components/ToolCall";
import { ThinkingBlock } from "./components/Thinking";
import { AnswerSection } from "./components/AnswerSection";
import { AskQuestionForm } from "./components/AskQuestion";
import { AttachmentStrip } from "./components/Attachments";
import { FilePreview } from "./components/FilePreview";
import { ConfirmSwitchModal } from "./components/ConfirmSwitchModal";
import { ConversationRail } from "./components/ConversationRail";
import { FirstLaunch } from "./components/FirstLaunch";
import { WorkspacePicker } from "./components/WorkspacePicker";
import {
  fileToAttachment,
  filesFromDataTransfer,
  imagesFromDataTransfer,
} from "./attachments";
import {
  createWorkspace,
  deleteConversation,
  getActiveWorkspace,
  listConversations,
  listWorkspaces,
  loadConversation,
  renameConversation,
  setActiveWorkspace,
} from "./workspaces";
import "./App.css";

function App() {
  const [blocks, setBlocks] = createSignal<DisplayBlock[]>([]);
  const [input, setInput] = createSignal("");
  const [attachments, setAttachments] = createSignal<Attachment[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [dragOver, setDragOver] = createSignal(false);
  const [previewPath, setPreviewPath] = createSignal<string | null>(null);
  // Bump to force the FilePreview to refetch from disk (e.g. after another Edit).
  const [previewRefresh, setPreviewRefresh] = createSignal(0);
  // Workspace state (Story 06).
  const [workspaces, setWorkspaces] = createSignal<Workspace[]>([]);
  const [activeWorkspace, setActiveWs] = createSignal<Workspace | null>(null);
  // Conversation rail (Story 07).
  const [conversations, setConversations] = createSignal<
    ConversationSummary[]
  >([]);
  const [activeConversationId, setActiveConversationId] = createSignal<
    string | null
  >(null);
  // Pending workspace switch awaiting user confirmation (Story 08).
  // Carries the target workspace id; the modal is shown while non-null.
  const [pendingSwitch, setPendingSwitch] = createSignal<Workspace | null>(
    null,
  );
  // Last user payload, kept so the Retry button can re-send after a stop.
  let lastPayload: { text: string; attachments: Attachment[] } | null = null;

  function openPreview(path: string) {
    setPreviewPath((prev) => {
      // If reopening the same path, refresh from disk so Edit/Write changes show.
      if (prev === path) setPreviewRefresh((n) => n + 1);
      return path;
    });
  }

  let unlisten: UnlistenFn | undefined;
  let logEl: HTMLDivElement | undefined;
  let fileInputEl: HTMLInputElement | undefined;

  async function refreshWorkspaces() {
    try {
      const [ws, active] = await Promise.all([
        listWorkspaces(),
        getActiveWorkspace(),
      ]);
      setWorkspaces(ws);
      setActiveWs(active);
      if (active) {
        await refreshConversations(active.id);
      } else {
        setConversations([]);
        setActiveConversationId(null);
      }
    } catch (err) {
      setBlocks((prev) => [...prev, { kind: "error", text: String(err) }]);
    }
  }

  async function refreshConversations(workspaceId: string) {
    try {
      const list = await listConversations(workspaceId);
      setConversations(list);
    } catch (err) {
      setBlocks((prev) => [...prev, { kind: "error", text: String(err) }]);
    }
  }

  async function onSelectConversation(id: string) {
    const ws = activeWorkspace();
    if (!ws) return;
    // End any in-flight session first — we don't want events from a
    // prior conversation leaking into the one being opened.
    try {
      await invoke("end_session");
    } catch {
      /* no session to end is fine */
    }
    setBusy(false);
    try {
      const loaded = await loadConversation(ws.id, id);
      const replayed: DisplayBlock[] = [];
      for (const ev of loaded.events) {
        replayed.push(...eventToBlocks(ev));
      }
      if (loaded.truncated_at_line != null) {
        replayed.push({
          kind: "error",
          text: `Conversation log was truncated at line ${loaded.truncated_at_line}; earlier events shown above.`,
        });
      }
      setBlocks(replayed);
      setActiveConversationId(id);
    } catch (err) {
      setBlocks([{ kind: "error", text: String(err) }]);
    }
  }

  async function onRenameConversation(id: string, currentTitle: string) {
    const newTitle = window.prompt("Rename conversation", currentTitle);
    if (!newTitle || !newTitle.trim()) return;
    const ws = activeWorkspace();
    if (!ws) return;
    try {
      await renameConversation(ws.id, id, newTitle.trim());
      await refreshConversations(ws.id);
    } catch (err) {
      setBlocks((prev) => [...prev, { kind: "error", text: String(err) }]);
    }
  }

  async function onDeleteConversation(id: string) {
    if (!window.confirm("Delete this conversation? This can't be undone.")) {
      return;
    }
    const ws = activeWorkspace();
    if (!ws) return;
    try {
      await deleteConversation(ws.id, id);
      await refreshConversations(ws.id);
      if (activeConversationId() === id) {
        setBlocks([]);
        setActiveConversationId(null);
      }
    } catch (err) {
      setBlocks((prev) => [...prev, { kind: "error", text: String(err) }]);
    }
  }

  async function onCreateFirstWorkspace(name: string, path: string) {
    const ws = await createWorkspace(name, path);
    await setActiveWorkspace(ws.id);
    await refreshWorkspaces();
    return ws;
  }

  async function performSwitchWorkspace(id: string) {
    try {
      await setActiveWorkspace(id);
      // Switching ends the current conversation (matches "+ New").
      setBlocks([]);
      setInput("");
      setAttachments([]);
      setBusy(false);
      setPreviewPath(null);
      await refreshWorkspaces();
    } catch (err) {
      setBlocks((prev) => [...prev, { kind: "error", text: String(err) }]);
    }
  }

  async function onSwitchWorkspace(id: string) {
    if (busy()) {
      // Defer behind a confirmation; the modal calls performSwitchWorkspace.
      const target = workspaces().find((w) => w.id === id);
      if (target) setPendingSwitch(target);
      return;
    }
    await performSwitchWorkspace(id);
  }

  // Auto-dismiss the switch-confirmation modal when the in-flight turn
  // ends on its own (no point asking the user about something that no
  // longer needs cancelling). Run the switch directly in that case.
  createEffect(() => {
    const target = pendingSwitch();
    if (target && !busy()) {
      setPendingSwitch(null);
      void performSwitchWorkspace(target.id);
    }
  });

  onMount(async () => {
    await refreshWorkspaces();
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
    setPreviewPath(null);
    setActiveConversationId(null);
    invoke("end_session").catch(() => {
      /* session may not have been started — fine */
    });
  }

  function stopTurn() {
    setBusy(false);
    // Mark the most recent user text block as cancelled so the inline
    // "stopped" badge + retry button render next to it.
    setBlocks((prev) => {
      const next = [...prev];
      for (let i = next.length - 1; i >= 0; i--) {
        const b = next[i];
        if (b.kind === "text" && b.role === "user" && !b.cancelled) {
          next[i] = { ...b, cancelled: true };
          break;
        }
      }
      return next;
    });
    invoke("cancel_turn").catch(() => {
      /* nothing to cancel — fine */
    });
  }

  async function onDrop(e: DragEvent) {
    e.preventDefault();
    setDragOver(false);
    const files = filesFromDataTransfer(e.dataTransfer);
    if (files.length === 0) return;
    await addFiles(files);
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

    lastPayload = { text, attachments: atts };

    // Render the user's message locally before clearing.
    setBlocks((prev) => {
      const next: DisplayBlock[] = [...prev];
      for (const a of atts) {
        if (a.kind === "image") {
          next.push({
            kind: "image",
            role: "user",
            mimeType: a.mimeType,
            data: a.data,
            alt: a.name,
          });
        } else {
          next.push({
            kind: "file",
            role: "user",
            name: a.name,
            mimeType: a.mimeType,
            size: a.size,
          });
        }
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
        attachments: atts.map(toBackendAttachment),
      });
    } catch (err) {
      setBlocks((prev) => [...prev, { kind: "error", text: String(err) }]);
      setBusy(false);
    }
  }

  function retryLastTurn() {
    if (!lastPayload || busy()) return;
    // Un-mark the most recent cancelled user text block.
    setBlocks((prev) => {
      const next = [...prev];
      for (let i = next.length - 1; i >= 0; i--) {
        const b = next[i];
        if (b.kind === "text" && b.role === "user" && b.cancelled) {
          next[i] = { ...b, cancelled: false };
          break;
        }
      }
      return next;
    });
    const payload = lastPayload;
    setBusy(true);
    invoke("send_message", {
      text: payload.text,
      attachments: payload.attachments.map(toBackendAttachment),
    }).catch((err) => {
      setBlocks((prev) => [...prev, { kind: "error", text: String(err) }]);
      setBusy(false);
    });
  }

  function toBackendAttachment(a: Attachment) {
    if (a.kind === "image") {
      return { kind: "image", mediaType: a.mimeType, data: a.data, name: a.name };
    }
    if (a.kind === "pdf") {
      return { kind: "pdf", mediaType: a.mimeType, data: a.data, name: a.name };
    }
    return { kind: "text", mediaType: a.mimeType, text: a.text, name: a.name };
  }

  return (
    <main
      class="app"
      classList={{
        "with-preview": !!previewPath(),
        "with-rail": !!activeWorkspace(),
      }}
    >
      <header class="header">
        <h1>linux coworker</h1>
        <Show
          when={activeWorkspace()}
          fallback={<span class="hint">Claude Code · Linux desktop</span>}
        >
          <WorkspacePicker
            active={activeWorkspace()!}
            workspaces={workspaces()}
            onSwitch={onSwitchWorkspace}
            onCreate={() => {
              // Manage flow is still TODO; for now fall through to the
              // same first-launch dialog rendered when there are no
              // workspaces. Story 06 leaves a follow-up to wire a real
              // "Manage workspaces" modal.
              setWorkspaces([]);
              setActiveWs(null);
            }}
            onManage={() => {
              // No-op for now; future story.
            }}
          />
        </Show>
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

      <div class="content">
      <Show when={activeWorkspace()}>
        <ConversationRail
          conversations={conversations()}
          activeConversationId={activeConversationId()}
          onSelect={onSelectConversation}
          onRename={onRenameConversation}
          onDelete={onDeleteConversation}
        />
      </Show>
      <section class="chat-pane">
      <Show
        when={activeWorkspace()}
        fallback={<FirstLaunch onCreate={onCreateFirstWorkspace} />}
      >
      <div class="log" ref={logEl}>
        <Show when={blocks().length === 0}>
          <div class="empty">
            Ask the agent to do something on your machine. It can run commands,
            edit files, take a screenshot, click, and type.
            <br />
            <span class="empty-hint">
              Paste an image, drop files (images, PDFs, text/code), or click 📎.
            </span>
          </div>
        </Show>
        <For each={blocks()}>
          {(b) => (
            <BlockView
              block={b}
              onAskSubmit={submitAskAnswers}
              onRetry={retryLastTurn}
              onPreview={openPreview}
            />
          )}
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
            title="Attach file (image, PDF, text/code)"
            onClick={() => fileInputEl?.click()}
          >
            📎
          </button>
          <input
            ref={fileInputEl}
            type="file"
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
            placeholder="Tell the agent what to do…  (Shift+Enter for newline · paste images · drop files)"
            rows={3}
          />
          <button
            type="button"
            class="send-btn"
            classList={{ stop: busy() }}
            title={busy() ? "Stop the current turn" : "Send"}
            onClick={(e) => (busy() ? stopTurn() : send(e))}
          >
            <Show when={busy()} fallback={"Send"}>
              <span class="stop-glyph" aria-hidden="true">
                ■
              </span>
              <span>Stop</span>
            </Show>
          </button>
        </div>
      </form>
      </Show>
      </section>
      <Show when={previewPath()}>
        <FilePreview
          path={previewPath()!}
          refreshKey={previewRefresh()}
          onClose={() => setPreviewPath(null)}
        />
      </Show>
      </div>
      <Show when={pendingSwitch()}>
        <ConfirmSwitchModal
          targetName={pendingSwitch()!.name}
          onCancel={() => setPendingSwitch(null)}
          onConfirm={() => {
            const target = pendingSwitch()!;
            setPendingSwitch(null);
            void performSwitchWorkspace(target.id);
          }}
        />
      </Show>
    </main>
  );
}

function BlockView(props: {
  block: DisplayBlock;
  onAskSubmit: (id: string, lines: string[]) => void;
  onRetry: () => void;
  onPreview: (path: string) => void;
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
                <Show
                  when={t.role === "assistant"}
                  fallback={
                    <div class="user-row">
                      <div class="user-text">{t.text}</div>
                      <Show when={t.cancelled}>
                        <div class="msg-status">
                          <span class="status-badge stopped">stopped</span>
                          <button
                            type="button"
                            class="retry-btn"
                            onClick={() => props.onRetry()}
                            title="Retry this message"
                          >
                            <span class="retry-glyph" aria-hidden="true">↻</span>
                            Retry
                          </button>
                        </div>
                      </Show>
                    </div>
                  }
                >
                  <div class="answer">
                    <AnswerSection text={t.text} />
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
      <Show when={b().kind === "file"}>
        {(() => {
          const f = b() as Extract<DisplayBlock, { kind: "file" }>;
          const label =
            f.mimeType === "application/pdf"
              ? "PDF"
              : f.name.includes(".")
                ? f.name.split(".").pop()!.toUpperCase().slice(0, 4)
                : "TXT";
          return (
            <div class={`msg ${f.role}`}>
              <div class="role">{f.role}</div>
              <div class="text">
                <div class="msg-file" title={`${f.name} · ${f.mimeType}`}>
                  <div class="msg-file-icon">{label}</div>
                  <div class="msg-file-meta">
                    <div class="msg-file-name">{f.name}</div>
                    <div class="msg-file-size">{formatBytes(f.size)}</div>
                  </div>
                </div>
              </div>
            </div>
          );
        })()}
      </Show>
      <Show when={b().kind === "tool_call"}>
        <ToolCallCard call={(b() as any).call} onPreview={props.onPreview} />
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

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export default App;
