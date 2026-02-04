import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useOpenDataDir } from "./useOpenDataDir";
import * as services from "@/services";

vi.mock("@/services");

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
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

describe("useOpenDataDir", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should call openDataDir service when mutated", async () => {
    vi.mocked(services.openDataDir).mockResolvedValue(undefined);

    const { result } = renderHook(() => useOpenDataDir(), {
      wrapper: createWrapper(),
    });

    result.current.mutate();

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(services.openDataDir).toHaveBeenCalledOnce();
  });

  it("should handle errors from openDataDir service", async () => {
    const mockError = new Error("Failed to open directory");
    vi.mocked(services.openDataDir).mockRejectedValue(mockError);

    const { result } = renderHook(() => useOpenDataDir(), {
      wrapper: createWrapper(),
    });

    result.current.mutate();

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toBe(mockError);
  });
});
