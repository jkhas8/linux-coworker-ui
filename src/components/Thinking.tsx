import { createSignal, Show } from "solid-js";

export function ThinkingBlock(props: { text: string; redacted?: boolean }) {
  const [open, setOpen] = createSignal(false);

  if (props.redacted) {
    return (
      <div class="thinking redacted">
        <div class="thinking-head">
          <span class="chev-static">›</span>
          <span class="thinking-dot" />
          <span class="thinking-label">Reasoning</span>
          <span class="thinking-preview muted">redacted by the model</span>
        </div>
      </div>
    );
  }

  const text = () => props.text ?? "";
  const lineCount = () =>
    text().split(/\r?\n/).filter((l) => l.trim()).length;
  const preview = () => {
    const firstLine = text().trim().split("\n").find((l) => l.trim()) ?? "";
    return firstLine.length > 110 ? firstLine.slice(0, 110) + "…" : firstLine;
  };

  function toggle(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    setOpen((o) => !o);
  }

  return (
    <div class="thinking" classList={{ open: open() }}>
      <button type="button" class="thinking-head" onClick={toggle}>
        <span class="chev">›</span>
        <span class="thinking-dot" />
        <span class="thinking-label">Reasoning</span>
        <span class="thinking-preview">
          <Show
            when={preview()}
            fallback={`${lineCount()} line${lineCount() === 1 ? "" : "s"} of internal reasoning`}
          >
            {preview()}
          </Show>
        </span>
      </button>
      <Show when={open()}>
        <pre class="thinking-body">{text()}</pre>
      </Show>
    </div>
  );
}
