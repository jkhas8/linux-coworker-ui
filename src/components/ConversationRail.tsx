// Left-rail list of conversations within the active workspace. Each item
// shows the title + a relative timestamp; a kebab menu offers rename and
// delete. Click dispatches to the host (App.tsx) for reopen (Story 09).

import { createSignal, For, onCleanup, Show } from "solid-js";
import type { ConversationSummary } from "../types";

export interface ConversationRailProps {
  conversations: ConversationSummary[];
  activeConversationId: string | null;
  onSelect: (id: string) => void;
  onRename: (id: string, currentTitle: string) => void;
  onDelete: (id: string) => void;
}

export function ConversationRail(props: ConversationRailProps) {
  return (
    <aside class="conv-rail" aria-label="Conversations">
      <Show
        when={props.conversations.length > 0}
        fallback={
          <div class="conv-rail-empty">No conversations yet</div>
        }
      >
        <ul class="conv-rail-list">
          <For each={props.conversations}>
            {(c) => (
              <ConversationItem
                item={c}
                active={c.id === props.activeConversationId}
                onSelect={() => props.onSelect(c.id)}
                onRename={() => props.onRename(c.id, c.title)}
                onDelete={() => props.onDelete(c.id)}
              />
            )}
          </For>
        </ul>
      </Show>
    </aside>
  );
}

interface ConversationItemProps {
  item: ConversationSummary;
  active: boolean;
  onSelect: () => void;
  onRename: () => void;
  onDelete: () => void;
}

function ConversationItem(props: ConversationItemProps) {
  const [menuOpen, setMenuOpen] = createSignal(false);
  let rootEl: HTMLLIElement | undefined;

  function onDocClick(e: MouseEvent) {
    if (!rootEl) return;
    if (!rootEl.contains(e.target as Node)) setMenuOpen(false);
  }

  function openMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    setMenuOpen(true);
    queueMicrotask(() => {
      document.addEventListener("click", onDocClick, true);
    });
  }

  function closeMenu() {
    setMenuOpen(false);
    document.removeEventListener("click", onDocClick, true);
  }

  onCleanup(() => document.removeEventListener("click", onDocClick, true));

  return (
    <li
      class="conv-rail-item"
      classList={{ active: props.active }}
      ref={rootEl}
      onContextMenu={openMenu}
    >
      <button
        type="button"
        class="conv-rail-item-button"
        onClick={() => props.onSelect()}
      >
        <span class="conv-rail-title">{props.item.title}</span>
        <span class="conv-rail-time">
          {formatRelativeTime(props.item.last_active_at)}
        </span>
      </button>
      <button
        type="button"
        class="conv-rail-kebab"
        aria-label="Conversation actions"
        onClick={(e) => {
          e.stopPropagation();
          openMenu(e);
        }}
      >
        ⋯
      </button>
      <Show when={menuOpen()}>
        <div class="conv-rail-menu" role="menu">
          <button
            type="button"
            class="conv-rail-menu-item"
            role="menuitem"
            onClick={() => {
              closeMenu();
              props.onRename();
            }}
          >
            Rename
          </button>
          <button
            type="button"
            class="conv-rail-menu-item danger"
            role="menuitem"
            onClick={() => {
              closeMenu();
              props.onDelete();
            }}
          >
            Delete
          </button>
        </div>
      </Show>
    </li>
  );
}

export function formatRelativeTime(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 0) return "just now";
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return "just now";
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.floor(hr / 24);
  if (day === 1) return "yesterday";
  if (day < 7) return `${day}d ago`;
  const date = new Date(ms);
  // YYYY-MM-DD for older entries — locale-stable.
  return date.toISOString().slice(0, 10);
}
