import { For, Show } from "solid-js";
import type { Attachment } from "../types";

export function AttachmentStrip(props: {
  items: Attachment[];
  onRemove: (id: string) => void;
}) {
  return (
    <div class="attach-strip" classList={{ empty: props.items.length === 0 }}>
      <For each={props.items}>
        {(a) => (
          <div
            class="attach-chip"
            classList={{ "attach-file": a.kind !== "image" }}
            title={`${a.name} · ${formatSize(a.size)}`}
          >
            <Show
              when={a.kind === "image"}
              fallback={
                <div class="attach-file-body">
                  <div class="attach-file-icon">{iconFor(a)}</div>
                  <div class="attach-file-meta">
                    <div class="attach-file-name">{a.name}</div>
                    <div class="attach-file-size">{formatSize(a.size)}</div>
                  </div>
                </div>
              }
            >
              <img
                src={(a as Extract<Attachment, { kind: "image" }>).dataUrl}
                alt={a.name}
              />
            </Show>
            <button
              type="button"
              class="attach-remove"
              aria-label="Remove attachment"
              onClick={() => props.onRemove(a.id)}
            >
              ×
            </button>
          </div>
        )}
      </For>
    </div>
  );
}

function iconFor(a: Attachment): string {
  if (a.kind === "pdf") return "PDF";
  // Extension as a tiny badge, falls back to "TXT".
  const i = a.name.lastIndexOf(".");
  if (i >= 0 && i < a.name.length - 1) {
    return a.name.slice(i + 1).toUpperCase().slice(0, 4);
  }
  return "TXT";
}

function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
