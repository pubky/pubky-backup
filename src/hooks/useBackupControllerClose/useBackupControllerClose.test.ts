import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useBackupControllerClose } from "./useBackupControllerClose";
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

describe("useBackupControllerClose", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should call backupControllerClose on mutate", async () => {
    vi.mocked(services.backupControllerClose).mockResolvedValue(undefined);

    const { result } = renderHook(() => useBackupControllerClose(), {
      wrapper: createWrapper(),
    });

    await act(async () => {
      await result.current.mutateAsync();
    });

    expect(services.backupControllerClose).toHaveBeenCalled();
  });

  it("should handle errors", async () => {
    const mockError = { type: "Internal", message: "Already closed" };
    vi.mocked(services.backupControllerClose).mockRejectedValue(mockError);

    const { result } = renderHook(() => useBackupControllerClose(), {
      wrapper: createWrapper(),
    });

    await expect(
      act(async () => {
        await result.current.mutateAsync();
      }),
    ).rejects.toEqual(mockError);
  });
});
