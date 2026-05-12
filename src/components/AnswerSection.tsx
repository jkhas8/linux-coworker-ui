import { createMemo, createSignal, Show } from "solid-js";
import { Markdown } from "./Markdown";

/// Some models prefix their answer with a leading run of markdown blockquotes
/// containing their own reasoning (`> Let me think… > checking X`). Hide that
/// behind a "Show reasoning" toggle so the user sees the actual answer first.
export function AnswerSection(props: { text: string }) {
  const split = createMemo(() => splitLeadingQuotes(props.text));
  const [open, setOpen] = createSignal(false);

  return (
    <>
      <Show when={split().thinking}>
        <div class="answer-thinking" classList={{ open: open() }}>
          <button
            type="button"
            class="answer-thinking-head"
            onClick={() => setOpen((o) => !o)}
          >
            <span class="chev">›</span>
            <span class="answer-thinking-label">
              {open() ? "Hide reasoning" : "Show reasoning"}
            </span>
          </button>
          <Show when={open()}>
            <div class="answer-thinking-body">
              <Markdown source={split().thinking} />
            </div>
          </Show>
        </div>
      </Show>
      <Show when={split().answer}>
        <Markdown source={split().answer} />
      </Show>
    </>
  );
}

function splitLeadingQuotes(s: string): { thinking: string; answer: string } {
  const lines = s.split("\n");
  let i = 0;
  const quoteLines: string[] = [];
  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();
    if (trimmed.startsWith(">")) {
      // Strip the leading '>' (and any single space after) so the inner
      // text renders as plain markdown when expanded.
      quoteLines.push(line.replace(/^\s*>\s?/, ""));
      i++;
    } else if (trimmed === "" && quoteLines.length > 0) {
      // Blank line in the middle of the leading quote run — keep going.
      quoteLines.push("");
      i++;
    } else {
      break;
    }
  }
  // Only treat as thinking if we matched at least one blockquote line; trim
  // trailing blanks from both halves.
  if (quoteLines.length === 0) {
    return { thinking: "", answer: s.trim() };
  }
  return {
    thinking: quoteLines.join("\n").trim(),
    answer: lines.slice(i).join("\n").trim(),
  };
}
