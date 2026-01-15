import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useForceSync } from "../useForceSync";
import * as tauriCommands from "@/services/tauri-commands";

vi.mock("@/services/tauri-commands");

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

describe("useForceSync", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should call forceSyncNow on mutate", async () => {
    vi.mocked(tauriCommands.forceSyncNow).mockResolvedValue(undefined);

    const { result } = renderHook(() => useForceSync(), {
      wrapper: createWrapper(),
    });

    await act(async () => {
      await result.current.mutateAsync();
    });

    expect(tauriCommands.forceSyncNow).toHaveBeenCalled();
  });

  it("should handle errors", async () => {
    const mockError = { type: "Backup", message: "Sync failed" };
    vi.mocked(tauriCommands.forceSyncNow).mockRejectedValue(mockError);

    const { result } = renderHook(() => useForceSync(), {
      wrapper: createWrapper(),
    });

    await expect(
      act(async () => {
        await result.current.mutateAsync();
      }),
    ).rejects.toEqual(mockError);
  });
});
