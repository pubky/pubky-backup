import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  render,
  screen,
  fireEvent,
  act,
  waitFor,
} from "@testing-library/react";
import { AddKeyForm } from "./AddKeyForm";
import { useUIStore } from "@/stores/uiStore";
import * as services from "@/services";

vi.mock("@/services", () => ({
  addKey: vi.fn(),
  signInNewKey: vi.fn(),
  cancelSignInNewKey: vi.fn(),
}));

const AUTHORIZATION_URL = "pubkyauth://example?secret=abc";

const clickSignIn = () =>
  fireEvent.click(
    screen.getByRole("button", { name: /sign in with pubky ring/i }),
  );

/** Make signInNewKey show the link, then wait for an approval that never comes. */
function mockSignInAwaitingApproval() {
  vi.mocked(services.signInNewKey).mockImplementation((onUrl) => {
    onUrl(AUTHORIZATION_URL);
    return new Promise(() => {});
  });
}

describe("AddKeyForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(services.cancelSignInNewKey).mockResolvedValue(undefined);
    useUIStore.setState({ lastPubky: null });
  });

  it("should offer signing in and entering a pubky", () => {
    render(<AddKeyForm onAdded={vi.fn()} />);

    expect(
      screen.getByRole("button", { name: /sign in with pubky ring/i }),
    ).toBeInTheDocument();
    expect(screen.getByText("Public and private data")).toBeInTheDocument();
    expect(screen.getByText("Public data only")).toBeInTheDocument();
  });

  describe("signing in", () => {
    it("should show the QR code once the sign-in link arrives", async () => {
      mockSignInAwaitingApproval();
      render(<AddKeyForm onAdded={vi.fn()} />);

      await act(async () => clickSignIn());

      expect(screen.getByTitle("Sign-in QR code")).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "Copy link" }),
      ).toBeInTheDocument();
    });

    it("should cancel a sign-in awaiting approval", async () => {
      mockSignInAwaitingApproval();
      render(<AddKeyForm onAdded={vi.fn()} />);
      await act(async () => clickSignIn());

      fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

      expect(services.cancelSignInNewKey).toHaveBeenCalledTimes(1);
      expect(
        screen.getByRole("button", { name: /sign in with pubky ring/i }),
      ).toBeInTheDocument();
    });

    it("should report the pubky that signed in", async () => {
      vi.mocked(services.signInNewKey).mockResolvedValue("signed-in-pubky");
      const onAdded = vi.fn();
      render(<AddKeyForm onAdded={onAdded} />);

      await act(async () => clickSignIn());

      expect(onAdded).toHaveBeenCalledWith("signed-in-pubky");
    });

    it("should show a failed sign-in and offer to try again", async () => {
      vi.mocked(services.signInNewKey).mockRejectedValue({
        type: "Internal",
        message: "request expired",
      });
      render(<AddKeyForm onAdded={vi.fn()} />);

      await act(async () => clickSignIn());

      expect(screen.getByRole("alert")).toHaveTextContent("request expired");
      expect(screen.getByRole("button", { name: "Try again" })).toBeEnabled();
    });
  });

  describe("entering a pubky", () => {
    const enterPubky = (value: string) =>
      fireEvent.change(screen.getByPlaceholderText(/paste a pubky/i), {
        target: { value },
      });
    const backUpButton = () =>
      screen.getByRole("button", { name: /back up public data/i });

    it("should only allow backing up once a pubky is entered", () => {
      render(<AddKeyForm onAdded={vi.fn()} />);
      expect(backUpButton()).toBeDisabled();

      enterPubky("some-pubky");

      expect(backUpButton()).toBeEnabled();
    });

    it("should back up the trimmed pubky and report it", async () => {
      vi.mocked(services.addKey).mockResolvedValue("pubkysome-pubky");
      const onAdded = vi.fn();
      render(<AddKeyForm onAdded={onAdded} />);

      enterPubky("  some-pubky  ");
      fireEvent.click(backUpButton());

      await waitFor(() => {
        expect(onAdded).toHaveBeenCalledWith("some-pubky");
      });
      expect(services.addKey).toHaveBeenCalledWith("some-pubky");
    });

    it("should show an error and stay open if the pubky can't be added", async () => {
      vi.mocked(services.addKey).mockRejectedValue({
        type: "InvalidPubkyFormat",
        message: "bad pubky",
      });
      const onAdded = vi.fn();
      render(<AddKeyForm onAdded={onAdded} />);

      enterPubky("bad");
      fireEvent.click(backUpButton());

      await waitFor(() => {
        expect(useUIStore.getState().toast.visible).toBe(true);
      });
      expect(onAdded).not.toHaveBeenCalled();
    });

    it("should disable both ways of adding while a pubky is being added", () => {
      vi.mocked(services.addKey).mockReturnValue(new Promise(() => {}));
      render(<AddKeyForm onAdded={vi.fn()} />);

      enterPubky("some-pubky");
      fireEvent.click(backUpButton());

      expect(backUpButton()).toBeDisabled();
      expect(
        screen.getByRole("button", { name: /sign in with pubky ring/i }),
      ).toBeDisabled();
    });
  });
});
