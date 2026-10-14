import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useNewKeySignIn } from "./useNewKeySignIn";
import { useUIStore } from "@/stores/uiStore";
import * as services from "@/services";

vi.mock("@/services", () => ({
  signInNewKey: vi.fn(),
  cancelSignInNewKey: vi.fn(),
}));

const AUTHORIZATION_URL = "pubkyauth://example?secret=abc";

interface PendingSignIn {
  showLink: (url: string) => void;
  approve: (pubky: string) => void;
  fail: (error: unknown) => void;
}

/** Make the next signInNewKey call wait until the test settles it. */
function mockPendingSignIn(): PendingSignIn {
  const pending: Partial<PendingSignIn> = {};
  vi.mocked(services.signInNewKey).mockImplementationOnce(
    (onAuthorizationUrl) =>
      new Promise<string>((resolve, reject) => {
        pending.showLink = onAuthorizationUrl;
        pending.approve = resolve;
        pending.fail = reject;
      }),
  );
  return pending as PendingSignIn;
}

describe("useNewKeySignIn", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(services.cancelSignInNewKey).mockResolvedValue(undefined);
    useUIStore.setState({ lastPubky: null });
  });

  it("should show the link, then report the pubky that signed in", async () => {
    const pending = mockPendingSignIn();
    const onSignedIn = vi.fn();
    const { result } = renderHook(() => useNewKeySignIn(onSignedIn));

    act(() => {
      void result.current.start();
    });
    expect(result.current.state).toEqual({ type: "Starting" });

    act(() => pending.showLink(AUTHORIZATION_URL));
    expect(result.current.state).toEqual({
      type: "AwaitingApproval",
      authorization_url: AUTHORIZATION_URL,
    });

    await act(async () => pending.approve("pubkyabc"));

    expect(onSignedIn).toHaveBeenCalledWith("abc");
    expect(useUIStore.getState().lastPubky).toBe("abc");
    expect(result.current.state).toEqual({ type: "Idle" });
  });

  it("should report why a sign-in failed", async () => {
    const pending = mockPendingSignIn();
    const onSignedIn = vi.fn();
    const { result } = renderHook(() => useNewKeySignIn(onSignedIn));

    act(() => {
      void result.current.start();
    });
    await act(async () =>
      pending.fail({ type: "Internal", message: "request expired" }),
    );

    expect(result.current.state).toEqual({
      type: "Failed",
      message: "request expired",
    });
    expect(onSignedIn).not.toHaveBeenCalled();
  });

  it("should ignore the outcome of a cancelled sign-in", async () => {
    const pending = mockPendingSignIn();
    const onSignedIn = vi.fn();
    const { result } = renderHook(() => useNewKeySignIn(onSignedIn));

    act(() => {
      void result.current.start();
    });
    act(() => pending.showLink(AUTHORIZATION_URL));
    act(() => result.current.cancel());

    expect(services.cancelSignInNewKey).toHaveBeenCalledTimes(1);
    expect(result.current.state).toEqual({ type: "Idle" });

    // The backend rejects the cancelled sign-in, which is no failure to show
    await act(async () =>
      pending.fail({ type: "Internal", message: "cancelled" }),
    );
    expect(result.current.state).toEqual({ type: "Idle" });
    expect(onSignedIn).not.toHaveBeenCalled();
  });

  it("should ignore the outcome of a replaced sign-in", async () => {
    const replaced = mockPendingSignIn();
    const current = mockPendingSignIn();
    const onSignedIn = vi.fn();
    const { result } = renderHook(() => useNewKeySignIn(onSignedIn));

    act(() => {
      void result.current.start();
    });
    act(() => {
      void result.current.start();
    });
    act(() => current.showLink(AUTHORIZATION_URL));
    await act(async () =>
      replaced.fail({ type: "Internal", message: "cancelled" }),
    );

    expect(result.current.state).toEqual({
      type: "AwaitingApproval",
      authorization_url: AUTHORIZATION_URL,
    });
  });

  it("should cancel a sign-in in progress on unmount", () => {
    mockPendingSignIn();
    const { result, unmount } = renderHook(() => useNewKeySignIn(vi.fn()));

    act(() => {
      void result.current.start();
    });
    unmount();

    expect(services.cancelSignInNewKey).toHaveBeenCalledTimes(1);
  });

  it("should not cancel anything on unmount when no sign-in is in progress", () => {
    const { unmount } = renderHook(() => useNewKeySignIn(vi.fn()));

    unmount();

    expect(services.cancelSignInNewKey).not.toHaveBeenCalled();
  });
});
