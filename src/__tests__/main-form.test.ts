import { describe, it, expect } from "vitest";
import { MainForm } from "../main-form";

describe("MainForm", () => {
  describe("displayPubky", () => {
    // Create an instance to access the private method through type assertion
    const mainForm = new MainForm();
    const displayPubky = (mainForm as any).displayPubky.bind(mainForm);

    it("should return ... for null input", () => {
      expect(displayPubky(null)).toBe("...");
    });

    it("should return full string if short enough", () => {
      // String with length <= prefix(5) + suffix(5) + ellipsis(3) = 13 chars
      expect(displayPubky("short")).toBe("short");
      expect(displayPubky("1234567890")).toBe("1234567890");
      expect(displayPubky("1234567890123")).toBe("1234567890123");
    });

    it("should truncate long pubkeys", () => {
      // String with length > 13 chars
      const longPubky = "abcdefghijklmnopqrstuvwxyz";
      const result = displayPubky(longPubky);
      expect(result).toBe("abcde...vwxyz");
    });

    it("should respect default prefix length of 5", () => {
      const longPubky = "0123456789abcdefghij";
      const result = displayPubky(longPubky);
      expect(result.startsWith("01234")).toBe(true);
      expect(result.includes("...")).toBe(true);
      expect(result.endsWith("fghij")).toBe(true);
    });

    it("should respect custom prefix length", () => {
      const longPubky = "0123456789abcdefghij";
      const result = displayPubky(longPubky, 8);
      expect(result.startsWith("01234567")).toBe(true);
      expect(result.includes("...")).toBe(true);
      expect(result.endsWith("fghij")).toBe(true);
    });

    it("should handle edge case at boundary length", () => {
      // Exactly 14 chars - should be truncated
      const boundaryPubky = "12345678901234";
      const result = displayPubky(boundaryPubky);
      expect(result).toBe("12345...01234");
    });
  });

  describe("formatFileSize", () => {
    const mainForm = new MainForm();
    const formatFileSize = (mainForm as any).formatFileSize.bind(mainForm);

    it("should format bytes correctly", () => {
      expect(formatFileSize(0)).toBe("0 B");
      expect(formatFileSize(1)).toBe("1 B");
      expect(formatFileSize(500)).toBe("500 B");
      expect(formatFileSize(1023)).toBe("1023 B");
    });

    it("should format kilobytes correctly", () => {
      expect(formatFileSize(1024)).toBe("1.00 KB");
      expect(formatFileSize(1536)).toBe("1.50 KB");
      expect(formatFileSize(10 * 1024)).toBe("10.0 KB");
      expect(formatFileSize(100 * 1024)).toBe("100.0 KB");
    });

    it("should format megabytes correctly", () => {
      expect(formatFileSize(1024 * 1024)).toBe("1.00 MB");
      expect(formatFileSize(1.5 * 1024 * 1024)).toBe("1.50 MB");
      expect(formatFileSize(10 * 1024 * 1024)).toBe("10.0 MB");
      expect(formatFileSize(100 * 1024 * 1024)).toBe("100.0 MB");
    });

    it("should format gigabytes correctly", () => {
      expect(formatFileSize(1024 * 1024 * 1024)).toBe("1.00 GB");
      expect(formatFileSize(1.5 * 1024 * 1024 * 1024)).toBe("1.50 GB");
      expect(formatFileSize(10 * 1024 * 1024 * 1024)).toBe("10.0 GB");
      expect(formatFileSize(100 * 1024 * 1024 * 1024)).toBe("100.0 GB");
    });

    it("should format terabytes correctly", () => {
      expect(formatFileSize(1024 * 1024 * 1024 * 1024)).toBe("1.00 TB");
      expect(formatFileSize(1.5 * 1024 * 1024 * 1024 * 1024)).toBe("1.50 TB");
      expect(formatFileSize(10 * 1024 * 1024 * 1024 * 1024)).toBe("10.0 TB");
      expect(formatFileSize(100 * 1024 * 1024 * 1024 * 1024)).toBe("100.0 TB");
    });

    it("should use correct decimal places - 0 for bytes", () => {
      expect(formatFileSize(1)).not.toContain(".");
      expect(formatFileSize(999)).not.toContain(".");
    });

    it("should use correct decimal places - 2 for < 10", () => {
      expect(formatFileSize(1024)).toBe("1.00 KB");
      expect(formatFileSize(9.99 * 1024)).toBe("9.99 KB");
    });

    it("should use correct decimal places - 1 for >= 10", () => {
      expect(formatFileSize(10 * 1024)).toBe("10.0 KB");
      expect(formatFileSize(99.9 * 1024)).toBe("99.9 KB");
      expect(formatFileSize(100 * 1024)).toBe("100.0 KB");
    });

    it("should handle very large numbers", () => {
      const petabyte = 1024 * 1024 * 1024 * 1024 * 1024;
      // Falls back to TB for very large numbers (index maxes out at 4 = TB)
      expect(formatFileSize(petabyte)).toBe("1.00 TB");
    });

    it("should handle fractional bytes gracefully", () => {
      // toFixed(0) rounds 512.5 to 513
      expect(formatFileSize(512.5)).toBe("513 B");
    });
  });

  describe("formatTimestamp", () => {
    const mainForm = new MainForm();
    const formatTimestamp = (mainForm as any).formatTimestamp.bind(mainForm);

    it("should format Unix timestamps correctly", () => {
      // Jan 1, 2024, 12:00:00 PM UTC (this will vary by timezone)
      const timestamp = 1704110400;
      const result = formatTimestamp(timestamp);
      expect(result).toMatch(/\d{1,2}:\d{2}:\d{2} (AM|PM)/);
    });

    it("should use 12-hour format with AM/PM", () => {
      // Create a timestamp for a specific time
      // 8:30:45 AM in local timezone
      const date = new Date("2024-01-01T08:30:45");
      const timestamp = Math.floor(date.getTime() / 1000);
      const result = formatTimestamp(timestamp);
      expect(result).toContain("AM");
      expect(result).toMatch(/8:30:45 AM/);
    });

    it("should use 12-hour format for afternoon times", () => {
      // 3:15:20 PM in local timezone
      const date = new Date("2024-01-01T15:15:20");
      const timestamp = Math.floor(date.getTime() / 1000);
      const result = formatTimestamp(timestamp);
      expect(result).toContain("PM");
      expect(result).toMatch(/3:15:20 PM/);
    });

    it("should pad minutes with leading zero", () => {
      // 2:05:30 PM
      const date = new Date("2024-01-01T14:05:30");
      const timestamp = Math.floor(date.getTime() / 1000);
      const result = formatTimestamp(timestamp);
      expect(result).toMatch(/2:05:30 PM/);
    });

    it("should pad seconds with leading zero", () => {
      // 3:30:05 PM
      const date = new Date("2024-01-01T15:30:05");
      const timestamp = Math.floor(date.getTime() / 1000);
      const result = formatTimestamp(timestamp);
      expect(result).toMatch(/3:30:05 PM/);
    });

    it("should handle midnight as 12:XX:XX AM", () => {
      // Midnight (00:00:00)
      const date = new Date("2024-01-01T00:00:00");
      const timestamp = Math.floor(date.getTime() / 1000);
      const result = formatTimestamp(timestamp);
      expect(result).toMatch(/12:00:00 AM/);
    });

    it("should handle noon as 12:XX:XX PM", () => {
      // Noon (12:00:00)
      const date = new Date("2024-01-01T12:00:00");
      const timestamp = Math.floor(date.getTime() / 1000);
      const result = formatTimestamp(timestamp);
      expect(result).toMatch(/12:00:00 PM/);
    });

    it("should handle 1 AM correctly", () => {
      // 1:00:00 AM
      const date = new Date("2024-01-01T01:00:00");
      const timestamp = Math.floor(date.getTime() / 1000);
      const result = formatTimestamp(timestamp);
      expect(result).toMatch(/1:00:00 AM/);
    });

    it("should handle 1 PM correctly", () => {
      // 1:00:00 PM (13:00:00)
      const date = new Date("2024-01-01T13:00:00");
      const timestamp = Math.floor(date.getTime() / 1000);
      const result = formatTimestamp(timestamp);
      expect(result).toMatch(/1:00:00 PM/);
    });

    it("should pad all single-digit values", () => {
      // 9:09:09 AM
      const date = new Date("2024-01-01T09:09:09");
      const timestamp = Math.floor(date.getTime() / 1000);
      const result = formatTimestamp(timestamp);
      expect(result).toMatch(/9:09:09 AM/);
    });
  });
});
