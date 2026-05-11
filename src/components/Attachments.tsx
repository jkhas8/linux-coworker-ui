import { For } from "solid-js";
import type { Attachment } from "../types";

export function AttachmentStrip(props: {
  items: Attachment[];
  onRemove: (id: string) => void;
}) {
  return (
    <div class="attach-strip" classList={{ empty: props.items.length === 0 }}>
      <For each={props.items}>
        {(a) => (
          <div class="attach-chip" title={`${a.name ?? "image"} · ${formatSize(a.size)}`}>
            <img src={a.dataUrl} alt={a.name ?? "attachment"} />
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

function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
