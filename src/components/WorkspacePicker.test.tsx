import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { WorkspacePicker } from "./WorkspacePicker";
import type { Workspace } from "../types";

function makeWorkspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    id: "ws-1",
    name: "alpha",
    path: "/home/me/alpha",
    last_used_at: 100,
    ...overrides,
  };
}

describe("<WorkspacePicker>", () => {
  it("shows the active workspace name as the trigger", () => {
    const active = makeWorkspace({ name: "alpha" });
    render(() => (
      <WorkspacePicker
        active={active}
        workspaces={[active]}
        onSwitch={vi.fn()}
        onCreate={vi.fn()}
        onManage={vi.fn()}
      />
    ));
    expect(screen.getByText("alpha")).toBeTruthy();
  });

  it("does not render the menu by default", () => {
    const active = makeWorkspace();
    render(() => (
      <WorkspacePicker
        active={active}
        workspaces={[active]}
        onSwitch={vi.fn()}
        onCreate={vi.fn()}
        onManage={vi.fn()}
      />
    ));
    expect(screen.queryByRole("menu")).toBeFalsy();
  });

  it("opens a menu with only action items when there is one workspace", () => {
    const active = makeWorkspace();
    render(() => (
      <WorkspacePicker
        active={active}
        workspaces={[active]}
        onSwitch={vi.fn()}
        onCreate={vi.fn()}
        onManage={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByText("alpha"));
    expect(screen.getByText("+ Create workspace")).toBeTruthy();
    expect(screen.getByText("Manage workspaces…")).toBeTruthy();
    expect(screen.queryByText("Switch to")).toBeFalsy();
  });

  it("lists other workspaces sorted by last_used_at desc", () => {
    const a = makeWorkspace({ id: "a", name: "alpha", last_used_at: 100 });
    const b = makeWorkspace({ id: "b", name: "bravo", last_used_at: 300 });
    const c = makeWorkspace({ id: "c", name: "charlie", last_used_at: 200 });
    render(() => (
      <WorkspacePicker
        active={a}
        workspaces={[a, b, c]}
        onSwitch={vi.fn()}
        onCreate={vi.fn()}
        onManage={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByText("alpha"));
    const items = screen
      .getAllByRole("menuitem")
      .filter((el) => el.textContent?.includes("alpha") || el.textContent?.includes("bravo") || el.textContent?.includes("charlie"));
    // First two items should be bravo then charlie (sorted desc by last_used_at).
    expect(items[0].textContent).toContain("bravo");
    expect(items[1].textContent).toContain("charlie");
  });

  it("invokes onSwitch when another workspace is picked", () => {
    const onSwitch = vi.fn();
    const a = makeWorkspace({ id: "a", name: "alpha" });
    const b = makeWorkspace({ id: "b", name: "bravo", last_used_at: 200 });
    render(() => (
      <WorkspacePicker
        active={a}
        workspaces={[a, b]}
        onSwitch={onSwitch}
        onCreate={vi.fn()}
        onManage={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByText("alpha"));
    fireEvent.click(screen.getByText("bravo"));
    expect(onSwitch).toHaveBeenCalledWith("b");
  });

  it("invokes onCreate when Create is clicked", () => {
    const onCreate = vi.fn();
    const a = makeWorkspace();
    render(() => (
      <WorkspacePicker
        active={a}
        workspaces={[a]}
        onSwitch={vi.fn()}
        onCreate={onCreate}
        onManage={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByText("alpha"));
    fireEvent.click(screen.getByText("+ Create workspace"));
    expect(onCreate).toHaveBeenCalled();
  });

  it("invokes onManage when Manage is clicked", () => {
    const onManage = vi.fn();
    const a = makeWorkspace();
    render(() => (
      <WorkspacePicker
        active={a}
        workspaces={[a]}
        onSwitch={vi.fn()}
        onCreate={vi.fn()}
        onManage={onManage}
      />
    ));
    fireEvent.click(screen.getByText("alpha"));
    fireEvent.click(screen.getByText("Manage workspaces…"));
    expect(onManage).toHaveBeenCalled();
  });

  it("closes the menu when the user clicks outside the picker", async () => {
    const a = makeWorkspace();
    render(() => (
      <WorkspacePicker
        active={a}
        workspaces={[a]}
        onSwitch={vi.fn()}
        onCreate={vi.fn()}
        onManage={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByText("alpha"));
    expect(screen.queryByRole("menu")).toBeTruthy();
    // The doc-click handler is registered in a microtask so the same
    // click doesn't close the freshly-opened menu — flush it.
    await Promise.resolve();
    // Click somewhere outside the picker.
    fireEvent.click(document.body);
    expect(screen.queryByRole("menu")).toBeFalsy();
  });
});
