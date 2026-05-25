import { describe, expect, it } from "vitest";
import { eventToBlocks } from "./stream";

describe("eventToBlocks", () => {
  it("suppresses resume_failure events (App handles them out-of-band)", () => {
    expect(eventToBlocks({ type: "resume_failure", exit_code: 1 })).toEqual([]);
  });

  it("returns [] for stream_event and rate_limit_event", () => {
    expect(eventToBlocks({ type: "stream_event" })).toEqual([]);
    expect(eventToBlocks({ type: "rate_limit_event" })).toEqual([]);
  });

  it("renders stderr as an error block", () => {
    expect(eventToBlocks({ type: "stderr", text: "boom" })).toEqual([
      { kind: "error", text: "boom" },
    ]);
  });
});
