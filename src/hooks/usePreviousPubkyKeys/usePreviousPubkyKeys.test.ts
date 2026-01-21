import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { usePreviousPubkyKeys } from "./usePreviousPubkyKeys";
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

describe("usePreviousPubkyKeys", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should fetch previous pubky keys", async () => {
    const mockKeys = ["pk:key1", "pk:key2", "pk:key3"];
    vi.mocked(services.getPreviousPubkyKeys).mockResolvedValue(mockKeys);

    const { result } = renderHook(() => usePreviousPubkyKeys(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual(mockKeys);
    expect(services.getPreviousPubkyKeys).toHaveBeenCalled();
  });

  it("should handle empty keys list", async () => {
    vi.mocked(services.getPreviousPubkyKeys).mockResolvedValue([]);

    const { result } = renderHook(() => usePreviousPubkyKeys(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual([]);
  });

  it("should handle errors", async () => {
    const mockError = { type: "Storage", message: "Read failed" };
    vi.mocked(services.getPreviousPubkyKeys).mockRejectedValue(mockError);

    const { result } = renderHook(() => usePreviousPubkyKeys(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(mockError);
  });
});
