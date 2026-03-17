import { useEffect, useRef, type RefObject } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";

const WINDOW_WIDTH = 480;
const MIN_HEIGHT = 300;
const MAX_HEIGHT = 800;
const RESIZE_DEBOUNCE_MS = 50;

/**
 * Observes the height of a container element and resizes the Tauri window to fit.
 * Width is kept fixed at 520px. Height adjusts to content within min/max bounds.
 */
export function useAutoWindowResize(): RefObject<HTMLDivElement | null> {
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let timeoutId: ReturnType<typeof setTimeout> | null = null;
    let lastHeight = 0;

    const resizeWindow = (height: number) => {
      const clamped = Math.min(
        Math.max(Math.ceil(height), MIN_HEIGHT),
        MAX_HEIGHT,
      );
      if (clamped === lastHeight) return;
      lastHeight = clamped;

      const appWindow = getCurrentWindow();
      void appWindow.setSize(new LogicalSize(WINDOW_WIDTH, clamped));
    };

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const height =
          entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height;

        if (timeoutId !== null) clearTimeout(timeoutId);
        timeoutId = setTimeout(() => resizeWindow(height), RESIZE_DEBOUNCE_MS);
      }
    });

    observer.observe(container);

    // Initial resize
    resizeWindow(container.getBoundingClientRect().height);

    return () => {
      observer.disconnect();
      if (timeoutId !== null) clearTimeout(timeoutId);
    };
  }, []);

  return containerRef;
}
