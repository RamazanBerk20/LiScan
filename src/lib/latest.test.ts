import { describe, expect, it } from "vitest";
import { createLatest } from "./latest";

describe("createLatest", () => {
  it("lets only the newest request's reply through", () => {
    const latest = createLatest();
    const first = latest.next();
    const second = latest.next();
    // The first reply arrives last and must be dropped.
    expect(latest.isCurrent(second)).toBe(true);
    expect(latest.isCurrent(first)).toBe(false);
  });
});
