import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { AddKeyForm, type AddKeyFormProps } from "./AddKeyForm";
import { useUIStore } from "@/stores/uiStore";
import * as services from "@/services";

vi.mock("@/services", () => ({
  signInNewKey: vi.fn(),
  cancelSignInNewKey: vi.fn(),
}));

const AUTHORIZATION_URL = "pubkyauth://example?secret=abc";

function renderForm(props: Partial<AddKeyFormProps> = {}) {
  const defaults: AddKeyFormProps = {
    onSignedIn: vi.fn(),
    pubkyValue: "",
    onPubkyChange: vi.fn(),
    onSubmitPubky: vi.fn(),
    isAddingPubky: false,
  };
  return render(<AddKeyForm {...defaults} {...props} />);
}

describe("AddKeyForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(services.cancelSignInNewKey).mockResolvedValue(undefined);
    useUIStore.setState({ lastPubky: null });
  });

  it("should offer signing in and entering a pubky", () => {
    renderForm();

    expect(
      screen.getByRole("button", { name: /sign in with pubky ring/i }),
    ).toBeInTheDocument();
    expect(screen.getByText("Public and private data")).toBeInTheDocument();
    expect(screen.getByText("Public data only")).toBeInTheDocument();
  });

  it("should show the QR code once the sign-in link arrives", async () => {
    vi.mocked(services.signInNewKey).mockImplementation((onUrl) => {
      onUrl(AUTHORIZATION_URL);
      return new Promise(() => {});
    });
    renderForm();

    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: /sign in with pubky ring/i }),
      );
    });

    expect(screen.getByTitle("Sign-in QR code")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Copy link" }),
    ).toBeInTheDocument();
  });

  it("should cancel a sign-in awaiting approval", async () => {
    vi.mocked(services.signInNewKey).mockImplementation((onUrl) => {
      onUrl(AUTHORIZATION_URL);
      return new Promise(() => {});
    });
    renderForm();
    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: /sign in with pubky ring/i }),
      );
    });

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(services.cancelSignInNewKey).toHaveBeenCalledTimes(1);
    expect(
      screen.getByRole("button", { name: /sign in with pubky ring/i }),
    ).toBeInTheDocument();
  });

  it("should report the pubky that signed in", async () => {
    vi.mocked(services.signInNewKey).mockResolvedValue("signed-in-pubky");
    const onSignedIn = vi.fn();
    renderForm({ onSignedIn });

    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: /sign in with pubky ring/i }),
      );
    });

    expect(onSignedIn).toHaveBeenCalledWith("signed-in-pubky");
  });

  it("should show a failed sign-in and offer to try again", async () => {
    vi.mocked(services.signInNewKey).mockRejectedValue({
      type: "Internal",
      message: "request expired",
    });
    renderForm();

    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: /sign in with pubky ring/i }),
      );
    });

    expect(screen.getByRole("alert")).toHaveTextContent("request expired");
    expect(screen.getByRole("button", { name: "Try again" })).toBeEnabled();
  });

  it("should submit the entered pubky for a public-only backup", () => {
    const onSubmitPubky = vi.fn();
    renderForm({ pubkyValue: "some-pubky", onSubmitPubky });

    fireEvent.click(
      screen.getByRole("button", { name: /back up public data/i }),
    );

    expect(onSubmitPubky).toHaveBeenCalledWith("some-pubky");
  });

  it("should disable both ways of adding while a pubky is being added", () => {
    renderForm({ pubkyValue: "some-pubky", isAddingPubky: true });

    expect(
      screen.getByRole("button", { name: /back up public data/i }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: /sign in with pubky ring/i }),
    ).toBeDisabled();
  });
});
