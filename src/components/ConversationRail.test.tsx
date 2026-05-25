import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { ConversationRail, formatRelativeTime } from "./ConversationRail";
import type { ConversationSummary } from "../types";

function makeConv(overrides: Partial<ConversationSummary> = {}): ConversationSummary {
  return {
    id: "c-1",
    title: "First chat",
    started_at: 0,
    last_active_at: Date.now(),
    claude_session_id: null,
    title_pinned: false,
    ...overrides,
  };
}

describe("formatRelativeTime", () => {
  it("returns 'just now' for recent times", () => {
    expect(formatRelativeTime(Date.now())).toBe("just now");
    expect(formatRelativeTime(Date.now() - 30_000)).toBe("just now");
  });

  it("returns 'just now' for future timestamps (clock skew)", () => {
    expect(formatRelativeTime(Date.now() + 10_000)).toBe("just now");
  });

  it("formats minutes / hours / days", () => {
    expect(formatRelativeTime(Date.now() - 5 * 60_000)).toBe("5m ago");
    expect(formatRelativeTime(Date.now() - 3 * 60 * 60_000)).toBe("3h ago");
    expect(formatRelativeTime(Date.now() - 24 * 60 * 60_000)).toBe("yesterday");
    expect(formatRelativeTime(Date.now() - 3 * 24 * 60 * 60_000)).toBe("3d ago");
  });

  it("falls back to ISO date for older entries", () => {
    const tenDaysAgo = Date.now() - 10 * 24 * 60 * 60_000;
    const out = formatRelativeTime(tenDaysAgo);
    expect(out).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe("<ConversationRail>", () => {
  it("shows empty state when there are no conversations", () => {
    render(() => (
      <ConversationRail
        conversations={[]}
        activeConversationId={null}
        onSelect={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
      />
    ));
    expect(screen.getByText("No conversations yet")).toBeTruthy();
  });

  it("renders one item per conversation with the title visible", () => {
    render(() => (
      <ConversationRail
        conversations={[
          makeConv({ id: "a", title: "First" }),
          makeConv({ id: "b", title: "Second" }),
        ]}
        activeConversationId={null}
        onSelect={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
      />
    ));
    expect(screen.getByText("First")).toBeTruthy();
    expect(screen.getByText("Second")).toBeTruthy();
  });

  it("invokes onSelect with the conversation id when clicked", () => {
    const onSelect = vi.fn();
    render(() => (
      <ConversationRail
        conversations={[makeConv({ id: "a", title: "First" })]}
        activeConversationId={null}
        onSelect={onSelect}
        onRename={vi.fn()}
        onDelete={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByText("First"));
    expect(onSelect).toHaveBeenCalledWith("a");
  });

  it("opens a kebab menu with Rename + Delete", () => {
    const onRename = vi.fn();
    const onDelete = vi.fn();
    render(() => (
      <ConversationRail
        conversations={[makeConv({ id: "a", title: "First" })]}
        activeConversationId={null}
        onSelect={vi.fn()}
        onRename={onRename}
        onDelete={onDelete}
      />
    ));
    fireEvent.click(screen.getByLabelText("Conversation actions"));
    fireEvent.click(screen.getByText("Rename"));
    expect(onRename).toHaveBeenCalledWith("a", "First");

    // Reopen and click Delete.
    fireEvent.click(screen.getByLabelText("Conversation actions"));
    fireEvent.click(screen.getByText("Delete"));
    expect(onDelete).toHaveBeenCalledWith("a");
  });

  it("highlights the active conversation", () => {
    render(() => (
      <ConversationRail
        conversations={[
          makeConv({ id: "a", title: "First" }),
          makeConv({ id: "b", title: "Second" }),
        ]}
        activeConversationId="b"
        onSelect={vi.fn()}
        onRename={vi.fn()}
        onDelete={vi.fn()}
      />
    ));
    const items = document.querySelectorAll(".conv-rail-item");
    expect(items[0].classList.contains("active")).toBe(false);
    expect(items[1].classList.contains("active")).toBe(true);
  });
});
