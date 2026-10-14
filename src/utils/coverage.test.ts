import { describe, it, expect } from "vitest";
import { backupCoverage } from "./coverage";
import type { AuthStatus } from "@/stores/uiStore";

describe("backupCoverage", () => {
  it.each<[AuthStatus, ReturnType<typeof backupCoverage>]>([
    [{ type: "SignedIn" }, "public-and-private"],
    [{ type: "SessionExpired" }, "private-paused"],
    [{ type: "SignedOut" }, "public-only"],
    [
      { type: "AwaitingApproval", authorization_url: "pubkyauth://x" },
      "public-only",
    ],
    [{ type: "SignInFailed", message: "expired" }, "public-only"],
  ])("should map %o to %s", (auth, expected) => {
    expect(backupCoverage(auth)).toBe(expected);
  });
});
