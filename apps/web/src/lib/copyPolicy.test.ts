import { describe, expect, it } from "vitest";
import {
  copyPolicyControlMethod,
  copyPolicyNetworkPassphrase,
  copyPolicyTransactionUrl,
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

describe("copyPolicyTransactionUrl", () => {
  it("uses the selected Stellar network and safely encodes the hash", () => {
    expect(copyPolicyTransactionUrl("testnet", "abc/123")).toBe(
      "https://lab.stellar.org/r/testnet/tx/abc%2F123",
    );
  });
});
