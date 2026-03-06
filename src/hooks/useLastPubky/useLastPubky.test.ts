import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { useLastPubky } from "./useLastPubky";
import * as services from "@/services";

vi.mock("@/services", () => ({
  getLastPubky: vi.fn(),
}));

describe("useLastPubky", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should fetch last pubky on mount and update state", async () => {
    vi.mocked(services.getLastPubky).mockResolvedValue("pk:my-last-pubky");

    const { result } = renderHook(() => useLastPubky());

    expect(result.current.isLoading).toBe(true);
    expect(result.current.lastPubky).toBeNull();

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.lastPubky).toBe("pk:my-last-pubky");
    expect(services.getLastPubky).toHaveBeenCalledTimes(1);
  });

  it("should handle null response", async () => {
    vi.mocked(services.getLastPubky).mockResolvedValue(null);

    const { result } = renderHook(() => useLastPubky());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.lastPubky).toBeNull();
  });

  it("should handle errors gracefully", async () => {
    vi.mocked(services.getLastPubky).mockRejectedValue(
      new Error("Failed to get last pubky"),
    );

    const { result } = renderHook(() => useLastPubky());

    await waitFor(() => {
      expect(result.current.isLoading).toBe(false);
    });

    expect(result.current.lastPubky).toBeNull();
  });
});
