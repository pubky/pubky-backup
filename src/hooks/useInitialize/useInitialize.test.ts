import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useInitialize } from "./useInitialize";
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
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe("useInitialize", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should call initAppState and backupControllerBegin on mutate", async () => {
    vi.mocked(services.initAppState).mockResolvedValue(undefined);
    vi.mocked(services.backupControllerBegin).mockResolvedValue(undefined);

    const { result } = renderHook(() => useInitialize(), {
      wrapper: createWrapper(),
    });

    await act(async () => {
      await result.current.mutateAsync({ pubkyValue: "pk:test123" });
    });

    expect(services.initAppState).toHaveBeenCalledWith("pk:test123");
    expect(services.backupControllerBegin).toHaveBeenCalled();
  });

  it("should handle initAppState errors", async () => {
    const mockError = { type: "InvalidPubkyFormat", message: "Bad format" };
    vi.mocked(services.initAppState).mockRejectedValue(mockError);

    const { result } = renderHook(() => useInitialize(), {
      wrapper: createWrapper(),
    });

    await expect(
      act(async () => {
        await result.current.mutateAsync({ pubkyValue: "bad-pubky" });
      }),
    ).rejects.toEqual(mockError);

    expect(services.backupControllerBegin).not.toHaveBeenCalled();
  });

  it("should handle backupControllerBegin errors", async () => {
    vi.mocked(services.initAppState).mockResolvedValue(undefined);
    const mockError = { type: "Internal", message: "Controller failed" };
    vi.mocked(services.backupControllerBegin).mockRejectedValue(mockError);

    const { result } = renderHook(() => useInitialize(), {
      wrapper: createWrapper(),
    });

    let thrownError: unknown;
    await act(async () => {
      try {
        await result.current.mutateAsync({ pubkyValue: "pk:test123" });
      } catch (e) {
        thrownError = e;
      }
    });

    expect(thrownError).toEqual(mockError);
    expect(services.initAppState).toHaveBeenCalledWith("pk:test123");
    expect(services.backupControllerBegin).toHaveBeenCalled();
  });

  it("should track pending state during mutation", async () => {
    let resolveInit: () => void;
    const initPromise = new Promise<void>((resolve) => {
      resolveInit = resolve;
    });
    vi.mocked(services.initAppState).mockReturnValue(initPromise);
    vi.mocked(services.backupControllerBegin).mockResolvedValue(undefined);

    const { result } = renderHook(() => useInitialize(), {
      wrapper: createWrapper(),
    });

    expect(result.current.isPending).toBe(false);

    act(() => {
      void result.current.mutateAsync({ pubkyValue: "pk:test123" });
    });

    await waitFor(() => {
      expect(result.current.isPending).toBe(true);
    });

    await act(async () => {
      resolveInit!();
    });

    await waitFor(() => {
      expect(result.current.isPending).toBe(false);
    });
  });
});
