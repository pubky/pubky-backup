import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { useCreateSnapshot } from "./useCreateSnapshot";
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

describe("useCreateSnapshot", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should call createSnapshot and return path on mutate", async () => {
    const mockPath = "/path/to/snapshot.zip";
    vi.mocked(services.createSnapshot).mockResolvedValue(mockPath);

    const { result } = renderHook(() => useCreateSnapshot(), {
      wrapper: createWrapper(),
    });

    let snapshotPath: string | undefined;
    await act(async () => {
      snapshotPath = await result.current.mutateAsync();
    });

    expect(services.createSnapshot).toHaveBeenCalled();
    expect(snapshotPath).toBe(mockPath);
  });

  it("should handle errors", async () => {
    const mockError = { type: "Storage", message: "Disk full" };
    vi.mocked(services.createSnapshot).mockRejectedValue(mockError);

    const { result } = renderHook(() => useCreateSnapshot(), {
      wrapper: createWrapper(),
    });

    await expect(
      act(async () => {
        await result.current.mutateAsync();
      }),
    ).rejects.toEqual(mockError);
  });
});
