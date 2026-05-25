import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { ConfirmSwitchModal } from "./ConfirmSwitchModal";

describe("<ConfirmSwitchModal>", () => {
  it("renders the target workspace name in the body copy", () => {
    render(() => (
      <ConfirmSwitchModal
        targetName="data-pipeline"
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />
    ));
    expect(screen.getByText("data-pipeline")).toBeTruthy();
    expect(screen.getByText("Switch workspace?")).toBeTruthy();
  });

  it("invokes onCancel when the Stay button is clicked", () => {
    const onCancel = vi.fn();
    render(() => (
      <ConfirmSwitchModal
        targetName="x"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />
    ));
    fireEvent.click(screen.getByText("Stay here"));
    expect(onCancel).toHaveBeenCalled();
  });

  it("invokes onConfirm when the Switch button is clicked", () => {
    const onConfirm = vi.fn();
    render(() => (
      <ConfirmSwitchModal
        targetName="x"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByText("Switch and cancel"));
    expect(onConfirm).toHaveBeenCalled();
  });

  it("cancels when the overlay backdrop is clicked", () => {
    const onCancel = vi.fn();
    render(() => (
      <ConfirmSwitchModal
        targetName="x"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />
    ));
    const overlay = document.querySelector(".confirm-switch-overlay")!;
    fireEvent.click(overlay);
    expect(onCancel).toHaveBeenCalled();
  });

  it("does not cancel when clicking inside the dialog", () => {
    const onCancel = vi.fn();
    render(() => (
      <ConfirmSwitchModal
        targetName="x"
        onConfirm={vi.fn()}
        onCancel={onCancel}
      />
    ));
    const dialog = document.querySelector(".confirm-switch-dialog")!;
    fireEvent.click(dialog);
    expect(onCancel).not.toHaveBeenCalled();
  });
});
