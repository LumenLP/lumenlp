import { describe, expect, it } from "vitest";
import {
  copyPolicyControlMethod,
  copyPolicyNetworkPassphrase,
} from "./copyPolicy";

describe("copyPolicyControlMethod", () => {
  it("maps UI controls only to owner-authorized contract methods", () => {
    expect(copyPolicyControlMethod("pause")).toBe("pause_session");
    expect(copyPolicyControlMethod("resume")).toBe("resume_session");
    expect(copyPolicyControlMethod("disarm")).toBe("disarm_session");
  });
});

describe("copyPolicyNetworkPassphrase", () => {
  it("uses the canonical Stellar network passphrases", () => {
    expect(copyPolicyNetworkPassphrase("testnet")).toBe(
      "Test SDF Network ; September 2015",
    );
    expect(copyPolicyNetworkPassphrase("public")).toBe(
      "Public Global Stellar Network ; September 2015",
    );
  });
});
