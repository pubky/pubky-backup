import { describe, it, expect } from "vitest";
import { render } from "@testing-library/react";
import {
  SearchIcon,
  BackIcon,
  CopyIcon,
  RefreshIcon,
  CheckIcon,
  ExternalLinkIcon,
  DatabaseIcon,
  ClockIcon,
  FolderIcon,
  SnapshotIcon,
} from "./Icons";

const icons = [
  { name: "SearchIcon", Component: SearchIcon, defaultSize: 16 },
  { name: "BackIcon", Component: BackIcon, defaultSize: 20 },
  { name: "CopyIcon", Component: CopyIcon, defaultSize: 20 },
  { name: "RefreshIcon", Component: RefreshIcon, defaultSize: 24 },
  { name: "CheckIcon", Component: CheckIcon, defaultSize: 24 },
  { name: "ExternalLinkIcon", Component: ExternalLinkIcon, defaultSize: 24 },
  { name: "DatabaseIcon", Component: DatabaseIcon, defaultSize: 24 },
  { name: "ClockIcon", Component: ClockIcon, defaultSize: 24 },
  { name: "FolderIcon", Component: FolderIcon, defaultSize: 24 },
  { name: "SnapshotIcon", Component: SnapshotIcon, defaultSize: 24 },
];

describe("Icons", () => {
  icons.forEach(({ name, Component, defaultSize }) => {
    describe(name, () => {
      it("should render an SVG element", () => {
        const { container } = render(<Component />);
        const svg = container.querySelector("svg");
        expect(svg).toBeInTheDocument();
      });

      it(`should use default size of ${defaultSize}`, () => {
        const { container } = render(<Component />);
        const svg = container.querySelector("svg");
        expect(svg).toHaveAttribute("width", String(defaultSize));
        expect(svg).toHaveAttribute("height", String(defaultSize));
      });

      it("should accept custom size", () => {
        const { container } = render(<Component size={32} />);
        const svg = container.querySelector("svg");
        expect(svg).toHaveAttribute("width", "32");
        expect(svg).toHaveAttribute("height", "32");
      });

      it("should have shrink-0 class by default", () => {
        const { container } = render(<Component />);
        const svg = container.querySelector("svg");
        expect(svg).toHaveClass("shrink-0");
      });

      it("should accept custom className", () => {
        const { container } = render(<Component className="text-red-500" />);
        const svg = container.querySelector("svg");
        expect(svg).toHaveClass("shrink-0");
        expect(svg).toHaveClass("text-red-500");
      });

      it("should use currentColor for stroke", () => {
        const { container } = render(<Component />);
        const path = container.querySelector("path");
        expect(path).toHaveAttribute("stroke", "currentColor");
      });
    });
  });
});
