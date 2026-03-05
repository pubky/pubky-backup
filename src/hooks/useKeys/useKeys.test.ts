import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useKeys } from "./useKeys";
import * as services from "@/services";

vi.mock("@/services");

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
      },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(
      QueryClientProvider,
      { client: queryClient },
      children,
    );
  };
}

describe("useKeys", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should fetch keys", async () => {
    const mockKeys = ["pk:key1", "pk:key2", "pk:key3"];
    vi.mocked(services.getKeys).mockResolvedValue(mockKeys);

    const { result } = renderHook(() => useKeys(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual(mockKeys);
    expect(services.getKeys).toHaveBeenCalled();
  });

  it("should handle empty keys list", async () => {
    vi.mocked(services.getKeys).mockResolvedValue([]);

    const { result } = renderHook(() => useKeys(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual([]);
  });

  it("should handle errors", async () => {
    const mockError = { type: "Storage", message: "Read failed" };
    vi.mocked(services.getKeys).mockRejectedValue(mockError);

    const { result } = renderHook(() => useKeys(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(mockError);
  });
});
