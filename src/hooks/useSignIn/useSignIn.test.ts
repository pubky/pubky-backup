import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSignIn } from "./useSignIn";
import * as services from "@/services";

vi.mock("@/services", () => ({
  startSignIn: vi.fn(),
  cancelSignIn: vi.fn(),
  signOut: vi.fn(),
}));

describe("useSignIn", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("should call startSignIn and toggle isPending", async () => {
    let resolvePromise: (url: string) => void;
    vi.mocked(services.startSignIn).mockReturnValue(
      new Promise<string>((resolve) => {
        resolvePromise = resolve;
      }),
    );

    const { result } = renderHook(() => useSignIn());
    expect(result.current.isPending).toBe(false);

    let signInPromise: Promise<void>;
    act(() => {
      signInPromise = result.current.startSignIn("test-pubky");
    });

    expect(result.current.isPending).toBe(true);

    await act(async () => {
      resolvePromise!("pubkyauth://example");
      await signInPromise;
    });

    expect(result.current.isPending).toBe(false);
    expect(services.startSignIn).toHaveBeenCalledWith("test-pubky");
  });

  it("should call cancelSignIn for the given pubky", async () => {
    vi.mocked(services.cancelSignIn).mockResolvedValue(undefined);
    const { result } = renderHook(() => useSignIn());

    await act(async () => {
      await result.current.cancelSignIn("test-pubky");
    });

    expect(services.cancelSignIn).toHaveBeenCalledWith("test-pubky");
  });

  it("should call signOut for the given pubky", async () => {
    vi.mocked(services.signOut).mockResolvedValue(undefined);
    const { result } = renderHook(() => useSignIn());

    await act(async () => {
      await result.current.signOut("test-pubky");
    });

    expect(services.signOut).toHaveBeenCalledWith("test-pubky");
  });

  it("should reset isPending and propagate errors", async () => {
    vi.mocked(services.startSignIn).mockRejectedValue(
      new Error("Sign-in failed"),
    );

    const { result } = renderHook(() => useSignIn());

    await expect(
      act(async () => {
        await result.current.startSignIn("test-pubky");
      }),
    ).rejects.toThrow("Sign-in failed");

    expect(result.current.isPending).toBe(false);
  });
});
