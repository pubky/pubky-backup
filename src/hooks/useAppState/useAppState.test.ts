import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useAppState } from "./useAppState";
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

describe("useAppState", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should fetch app state when enabled", async () => {
    const mockState = {
      pubky: "pk:test123",
      developer_mode: false,
      is_syncing: false,
      next_sync_time: 1672531200,
      data_dir_size: 1024,
      backup_controller_error: null,
    };

    vi.mocked(services.fetchState).mockResolvedValue(mockState);

    const { result } = renderHook(() => useAppState(true), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isSuccess).toBe(true);
    });

    expect(result.current.data).toEqual(mockState);
    expect(services.fetchState).toHaveBeenCalled();
  });

  it("should not fetch when disabled", async () => {
    const { result } = renderHook(() => useAppState(false), {
      wrapper: createWrapper(),
    });

    // Wait a tick to ensure no fetch happens
    await new Promise((resolve) => setTimeout(resolve, 50));

    expect(result.current.isFetching).toBe(false);
    expect(services.fetchState).not.toHaveBeenCalled();
  });

  it("should handle fetch errors", async () => {
    const mockError = { type: "Internal", message: "Connection failed" };
    vi.mocked(services.fetchState).mockRejectedValue(mockError);

    const { result } = renderHook(() => useAppState(true), {
      wrapper: createWrapper(),
    });

    await waitFor(() => {
      expect(result.current.isError).toBe(true);
    });

    expect(result.current.error).toEqual(mockError);
  });
});
