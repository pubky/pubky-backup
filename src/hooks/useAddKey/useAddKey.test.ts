import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useAddKey } from "./useAddKey";
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

describe("useAddKey", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should call addKey on mutate", async () => {
    vi.mocked(services.addKey).mockResolvedValue(undefined);

    const { result } = renderHook(() => useAddKey(), {
      wrapper: createWrapper(),
    });

    await act(async () => {
      await result.current.mutateAsync({ pubkyValue: "pk:test123" });
    });

    expect(services.addKey).toHaveBeenCalledWith("pk:test123");
  });

  it("should handle addKey errors", async () => {
    const mockError = { type: "InvalidPubkyFormat", message: "Bad format" };
    vi.mocked(services.addKey).mockRejectedValue(mockError);

    const { result } = renderHook(() => useAddKey(), {
      wrapper: createWrapper(),
    });

    await expect(
      act(async () => {
        await result.current.mutateAsync({ pubkyValue: "bad-pubky" });
      }),
    ).rejects.toEqual(mockError);
  });

  it("should track pending state during mutation", async () => {
    let resolveAddKey: () => void;
    const addKeyPromise = new Promise<void>((resolve) => {
      resolveAddKey = resolve;
    });
    vi.mocked(services.addKey).mockReturnValue(addKeyPromise);

    const { result } = renderHook(() => useAddKey(), {
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
      resolveAddKey!();
    });

    await waitFor(() => {
      expect(result.current.isPending).toBe(false);
    });
  });
});
