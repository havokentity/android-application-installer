import { useState, useCallback, useRef, useEffect } from "react";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

const DEFAULT_SIDE_WIDTH = 340;

export function useLayout() {
  const [layout, setLayout] = useState<"portrait" | "landscape">(() => {
    return (localStorage.getItem("layout") as "portrait" | "landscape") || "landscape";
  });
  const [sidePanelWidth, setSidePanelWidth] = useState<number>(() => {
    const saved = localStorage.getItem("landscapeWidth");
    return saved ? Number(saved) : DEFAULT_SIDE_WIDTH;
  });
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    return (localStorage.getItem("theme") as "dark" | "light") || "dark";
  });

  const dragging = useRef(false);
  const appRef = useRef<HTMLDivElement>(null);

  // Apply correct window size on first mount based on saved layout
  useEffect(() => {
    const win = getCurrentWindow();
    (async () => {
      if (layout === "landscape") {
        await win.setMinSize(new LogicalSize(1080, 520));
        await win.setSize(new LogicalSize(1280, 720));
      } else {
        await win.setMinSize(new LogicalSize(680, 520));
        await win.setSize(new LogicalSize(920, 740));
      }
      await win.center();
    })();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("theme", theme);
  }, [theme]);

  const toggleLayout = useCallback(async (mode: "portrait" | "landscape") => {
    const win = getCurrentWindow();
    if (mode === "landscape") {
      await win.setMinSize(new LogicalSize(1080, 520));
      await win.setSize(new LogicalSize(1280, 720));
      setSidePanelWidth(DEFAULT_SIDE_WIDTH);
      localStorage.setItem("landscapeWidth", String(DEFAULT_SIDE_WIDTH));
    } else {
      await win.setSize(new LogicalSize(920, 740));
      await win.setMinSize(new LogicalSize(680, 520));
      localStorage.removeItem("landscapeWidth");
    }
    await win.center();
    setLayout(mode);
    localStorage.setItem("layout", mode);
  }, []);

  const onDividerMouseDown = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";

    const onMouseMove = (ev: MouseEvent) => {
      if (!dragging.current || !appRef.current) return;
      const appRect = appRef.current.getBoundingClientRect();
      const newSideWidth = appRect.right - ev.clientX - 12;
      const clamped = Math.max(240, Math.min(newSideWidth, appRect.width - 400));
      setSidePanelWidth(clamped);
    };

    const onMouseUp = () => {
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      setSidePanelWidth((w) => {
        localStorage.setItem("landscapeWidth", String(w));
        return w;
      });
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, []);

  return {
    layout, theme, setTheme, sidePanelWidth,
    toggleLayout, onDividerMouseDown, appRef,
  };
}

