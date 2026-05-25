// Header workspace picker: shows the active workspace's name and opens
// a menu listing other workspaces (most-recently-used first), plus
// "Create workspace" and "Manage workspaces" entries.

import { createSignal, For, onCleanup, Show } from "solid-js";
import type { Workspace } from "../types";

export interface WorkspacePickerProps {
  active: Workspace;
  workspaces: Workspace[]; // includes the active one
  onSwitch: (id: string) => void | Promise<void>;
  onCreate: () => void;
  onManage: () => void;
}

export function WorkspacePicker(props: WorkspacePickerProps) {
  const [open, setOpen] = createSignal(false);

  // Most-recently-used first, but pull the active to the very top so the
  // checked entry is always visible without scrolling.
  const others = () =>
    props.workspaces
      .filter((w) => w.id !== props.active.id)
      .slice()
      .sort((a, b) => b.last_used_at - a.last_used_at);

  let rootEl: HTMLDivElement | undefined;

  function onDocClick(e: MouseEvent) {
    if (!rootEl) return;
    if (!rootEl.contains(e.target as Node)) setOpen(false);
  }

  function toggle() {
    if (open()) {
      setOpen(false);
      document.removeEventListener("click", onDocClick, true);
    } else {
      setOpen(true);
      // Defer so the same click that opens us doesn't immediately close.
      queueMicrotask(() => {
        document.addEventListener("click", onDocClick, true);
      });
    }
  }

  onCleanup(() => document.removeEventListener("click", onDocClick, true));

  function pick(id: string) {
    setOpen(false);
    document.removeEventListener("click", onDocClick, true);
    void props.onSwitch(id);
  }

  return (
    <div class="ws-picker" ref={rootEl}>
      <button
        type="button"
        class="ws-picker-trigger"
        title={props.active.path}
        aria-expanded={open() ? "true" : "false"}
        aria-haspopup="menu"
        onClick={toggle}
      >
        <span class="ws-picker-dot" aria-hidden="true" />
        <span class="ws-picker-name">{props.active.name}</span>
        <span class="ws-picker-chev" aria-hidden="true">
          ▾
        </span>
      </button>

      <Show when={open()}>
        <div class="ws-picker-menu" role="menu">
          <Show when={others().length > 0}>
            <div class="ws-picker-group-label">Switch to</div>
            <For each={others()}>
              {(w) => (
                <button
                  type="button"
                  class="ws-picker-item"
                  role="menuitem"
                  title={w.path}
                  onClick={() => pick(w.id)}
                >
                  <span class="ws-picker-item-name">{w.name}</span>
                  <span class="ws-picker-item-path">{w.path}</span>
                </button>
              )}
            </For>
            <div class="ws-picker-sep" role="separator" />
          </Show>
          <button
            type="button"
            class="ws-picker-item ws-picker-action"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              props.onCreate();
            }}
          >
            + Create workspace
          </button>
          <button
            type="button"
            class="ws-picker-item ws-picker-action"
            role="menuitem"
            onClick={() => {
              setOpen(false);
              props.onManage();
            }}
          >
            Manage workspaces…
          </button>
        </div>
      </Show>
    </div>
  );
}
