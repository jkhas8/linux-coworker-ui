import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { FirstLaunch } from "./FirstLaunch";
import type { Workspace } from "../types";

function makeWorkspace(overrides: Partial<Workspace> = {}): Workspace {
  return {
    id: "ws-1",
    name: "my-app",
    path: "/home/me/code/my-app",
    last_used_at: 0,
    ...overrides,
  };
}

describe("<FirstLaunch>", () => {
  it("renders the CTA initially, not the form", () => {
    render(() => <FirstLaunch onCreate={async () => makeWorkspace()} />);
    expect(screen.getByText("+ Create workspace")).toBeTruthy();
    expect(screen.queryByText("Create")).toBeFalsy();
  });

  it("opens the form when the CTA is clicked", () => {
    render(() => <FirstLaunch onCreate={async () => makeWorkspace()} />);
    fireEvent.click(screen.getByText("+ Create workspace"));
    expect(screen.getByPlaceholderText("my-app")).toBeTruthy();
    expect(screen.getByPlaceholderText("/home/you/code/my-app")).toBeTruthy();
  });

  it("calls onCreate with trimmed name + path and closes on success", async () => {
    const onCreate = vi.fn().mockResolvedValue(makeWorkspace());
    render(() => <FirstLaunch onCreate={onCreate} />);
    fireEvent.click(screen.getByText("+ Create workspace"));

    const nameInput = screen.getByPlaceholderText("my-app") as HTMLInputElement;
    const pathInput = screen.getByPlaceholderText(
      "/home/you/code/my-app",
    ) as HTMLInputElement;
    fireEvent.input(nameInput, { target: { value: "  my-app  " } });
    fireEvent.input(pathInput, {
      target: { value: "  /home/me/code/my-app  " },
    });

    const submit = screen.getByText("Create");
    fireEvent.click(submit);

    await vi.waitFor(() => {
      expect(onCreate).toHaveBeenCalledWith("my-app", "/home/me/code/my-app");
    });
    // CTA returns once the form closes.
    await vi.waitFor(() => {
      expect(screen.queryByText("+ Create workspace")).toBeTruthy();
    });
  });

  it("surfaces backend errors inline without closing the form", async () => {
    const onCreate = vi.fn().mockRejectedValue("workspace name 'x' already exists");
    render(() => <FirstLaunch onCreate={onCreate} />);
    fireEvent.click(screen.getByText("+ Create workspace"));
    fireEvent.input(screen.getByPlaceholderText("my-app"), {
      target: { value: "x" },
    });
    fireEvent.input(screen.getByPlaceholderText("/home/you/code/my-app"), {
      target: { value: "/tmp" },
    });
    fireEvent.click(screen.getByText("Create"));

    await vi.waitFor(() => {
      expect(screen.getByText(/already exists/)).toBeTruthy();
    });
    // Form should still be open.
    expect(screen.queryByPlaceholderText("my-app")).toBeTruthy();
  });

  it("cancels back to the CTA without calling onCreate", () => {
    const onCreate = vi.fn();
    render(() => <FirstLaunch onCreate={onCreate} />);
    fireEvent.click(screen.getByText("+ Create workspace"));
    fireEvent.click(screen.getByText("Cancel"));
    expect(onCreate).not.toHaveBeenCalled();
    expect(screen.getByText("+ Create workspace")).toBeTruthy();
  });
});
