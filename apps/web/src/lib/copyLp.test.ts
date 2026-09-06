import { describe, expect, it } from "vitest";
import { ApiError } from "./api";
import { formatCopyError } from "./copyLp";

describe("formatCopyError", () => {
  it("turns policy binding failures into an actionable message", () => {
    expect(
      formatCopyError(
        new ApiError("policy session is required", 409, "policy_session_missing"),
        "fallback",
      ),
    ).toContain("Bind an on-chain Soroban policy session");
  });

  it("explains why an unverified claim cannot be prepared", () => {
    expect(
      formatCopyError(
        new ApiError("claim reward token is not verified", 422, "claim_token_missing"),
        "fallback",
      ),
    ).toContain("verified reward token");
  });

  it("preserves plain API errors", () => {
    expect(formatCopyError(new Error("temporary upstream failure"), "fallback")).toBe(
      "temporary upstream failure",
    );
  });

  it("explains wallet challenge throttling", () => {
    expect(
      formatCopyError(
        new ApiError("wallet authentication challenge rate limit exceeded", 429, "auth_rate_limited"),
        "fallback",
      ),
    ).toContain("Wait a few seconds");
  });
});
