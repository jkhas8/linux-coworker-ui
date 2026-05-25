import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { ResumeFailureBanner } from "./ResumeFailureBanner";

describe("<ResumeFailureBanner>", () => {
  it("renders the recovery copy and both buttons", () => {
    render(() => (
      <ResumeFailureBanner
        onContinueFresh={vi.fn()}
        onDismiss={vi.fn()}
      />
    ));
    expect(
      screen.getByText(/This conversation can't be resumed/),
    ).toBeTruthy();
    expect(screen.getByText("Continue fresh")).toBeTruthy();
    expect(screen.getByText("Dismiss")).toBeTruthy();
  });

  it("invokes onContinueFresh when the primary action is clicked", () => {
    const onContinueFresh = vi.fn();
    render(() => (
      <ResumeFailureBanner
        onContinueFresh={onContinueFresh}
        onDismiss={vi.fn()}
      />
    ));
    fireEvent.click(screen.getByText("Continue fresh"));
    expect(onContinueFresh).toHaveBeenCalled();
  });

  it("invokes onDismiss when Dismiss is clicked", () => {
    const onDismiss = vi.fn();
    render(() => (
      <ResumeFailureBanner
        onContinueFresh={vi.fn()}
        onDismiss={onDismiss}
      />
    ));
    fireEvent.click(screen.getByText("Dismiss"));
    expect(onDismiss).toHaveBeenCalled();
  });

  it("has role=alert for assistive tech", () => {
    render(() => (
      <ResumeFailureBanner
        onContinueFresh={vi.fn()}
        onDismiss={vi.fn()}
      />
    ));
    expect(screen.getByRole("alert")).toBeTruthy();
  });
});
