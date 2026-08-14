import { describe, expect, it } from "vitest";
import { fmtUsd } from "./api";

describe("fmtUsd", () => {
  it("formats millions like lpagent", () => {
    expect(fmtUsd(1_217_341.73)).toBe("$1.22m");
  });

  it("formats thousands with k", () => {
    expect(fmtUsd(48_510)).toBe("$48.51k");
  });

  it("keeps small amounts with $ and decimals", () => {
    expect(fmtUsd(377.88)).toBe("$377.88");
  });

  it("handles negatives and null", () => {
    expect(fmtUsd(-1_500)).toBe("-$1.50k");
    expect(fmtUsd(null)).toBe("—");
  });
});
