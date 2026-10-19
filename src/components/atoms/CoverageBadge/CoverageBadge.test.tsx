import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { CoverageBadge } from "./CoverageBadge";

describe("CoverageBadge", () => {
  it.each([
    ["public-and-private", "Public + private"],
    ["public-only", "Public only"],
    ["private-paused", "Public only · sign-in expired"],
  ] as const)("should label %s coverage", (coverage, label) => {
    render(<CoverageBadge coverage={coverage} />);

    expect(screen.getByText(label)).toBeInTheDocument();
  });

  it("should mark a mirrored backup", () => {
    render(<CoverageBadge coverage="public-only" isMirrored />);

    expect(screen.getByText("Public only, mirrored")).toBeInTheDocument();
    expect(screen.getByTitle(/copied to a mirror/i)).toBeInTheDocument();
  });
});
