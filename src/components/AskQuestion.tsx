import { createSignal, For, Show } from "solid-js";
import type { AskQuestion } from "../types";

export function AskQuestionForm(props: {
  questions: AskQuestion[];
  submitted?: { lines: string[] };
  onSubmit: (lines: string[]) => void;
}) {
  // Per-question selected option indices.
  const [selections, setSelections] = createSignal<number[][]>(
    props.questions.map(() => []),
  );
  // Per-question "other" free-text answers.
  const [other, setOther] = createSignal<string[]>(props.questions.map(() => ""));

  function toggle(qi: number, oi: number) {
    if (props.submitted) return;
    setSelections((prev) => {
      const next = prev.map((s) => [...s]);
      const q = props.questions[qi];
      const idx = next[qi].indexOf(oi);
      if (q.multiSelect) {
        if (idx >= 0) next[qi].splice(idx, 1);
        else next[qi].push(oi);
      } else {
        next[qi] = [oi];
      }
      return next;
    });
  }

  function setOtherText(qi: number, t: string) {
    if (props.submitted) return;
    setOther((prev) => {
      const next = [...prev];
      next[qi] = t;
      return next;
    });
  }

  function submit() {
    const lines = props.questions.map((q, qi) => {
      const sel = selections()[qi] ?? [];
      const labels = sel.map((i) => q.options[i]?.label).filter(Boolean);
      const ot = (other()[qi] ?? "").trim();
      if (ot) labels.push(ot);
      const answer = labels.join(", ") || "(skipped)";
      return `**${q.question}** → ${answer}`;
    });
    props.onSubmit(lines);
  }

  return (
    <div class="ask-question" classList={{ done: !!props.submitted }}>
      <div class="aq-head">
        <span class="dot" />
        <span class="aq-title">Claude is asking</span>
      </div>
      <For each={props.questions}>
        {(q, qi) => (
          <div class="aq-question">
            <Show when={q.header}>
              <div class="aq-header-chip">{q.header}</div>
            </Show>
            <div class="aq-prompt">{q.question}</div>
            <div class="aq-options">
              <For each={q.options}>
                {(o, oi) => (
                  <label
                    class="aq-option"
                    classList={{
                      selected: (selections()[qi()] ?? []).includes(oi()),
                    }}
                  >
                    <input
                      type={q.multiSelect ? "checkbox" : "radio"}
                      name={`q-${props.questions === props.questions ? qi() : qi()}`}
                      checked={(selections()[qi()] ?? []).includes(oi())}
                      onChange={() => toggle(qi(), oi())}
                      disabled={!!props.submitted}
                    />
                    <span class="aq-option-text">
                      <span class="aq-label">{o.label}</span>
                      <Show when={o.description}>
                        <span class="aq-desc">{o.description}</span>
                      </Show>
                    </span>
                  </label>
                )}
              </For>
              <label class="aq-option aq-other">
                <span class="aq-label">Other:</span>
                <input
                  type="text"
                  class="aq-other-input"
                  placeholder="(type a custom answer)"
                  value={other()[qi()] ?? ""}
                  onInput={(e) => setOtherText(qi(), e.currentTarget.value)}
                  disabled={!!props.submitted}
                />
              </label>
            </div>
          </div>
        )}
      </For>
      <Show
        when={!props.submitted}
        fallback={
          <div class="aq-summary">
            <For each={props.submitted!.lines}>
              {(l) => <div class="aq-summary-line" innerHTML={renderInline(l)} />}
            </For>
          </div>
        }
      >
        <button type="button" class="aq-submit" onClick={submit}>
          Submit answers
        </button>
      </Show>
    </div>
  );
}

// Minimal inline markdown for `**bold**`; questions sometimes contain it.
function renderInline(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
}
