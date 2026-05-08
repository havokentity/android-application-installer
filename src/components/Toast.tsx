// ─── Toast Notification System ────────────────────────────────────────────────
import { useState, useCallback, useEffect, useRef } from "react";
import { CheckCircle, AlertTriangle, XCircle, Info, X } from "lucide-react";

export type ToastLevel = "success" | "error" | "warning" | "info";

export interface Toast {
  id: number;
  message: string;
  level: ToastLevel;
  exiting?: boolean;
}

let toastIdCounter = 0;

// Must match the duration of the `toast-slide-out` animation on
// `.toast-exit` in App.css so the element stays mounted long enough
// to play the fade-out before being removed from state.
const TOAST_EXIT_ANIMATION_MS = 300;

export function useToast(duration = 3500) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timersRef = useRef<Map<number, ReturnType<typeof setTimeout>>>(new Map());

  const removeToast = useCallback((id: number) => {
    // Mark as exiting for animation, then remove. The exit timer is
    // tracked in timersRef under the same id so the unmount cleanup
    // below clears it — otherwise a fast unmount could fire setState
    // on an unmounted hook.
    setToasts((prev) => prev.map((t) => t.id === id ? { ...t, exiting: true } : t));
    const existing = timersRef.current.get(id);
    if (existing) clearTimeout(existing);
    const exitTimer = setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
      timersRef.current.delete(id);
    }, TOAST_EXIT_ANIMATION_MS);
    timersRef.current.set(id, exitTimer);
  }, []);

  const addToast = useCallback((message: string, level: ToastLevel = "info") => {
    const id = ++toastIdCounter;
    setToasts((prev) => [...prev.slice(-4), { id, message, level }]); // keep max 5
    const timer = setTimeout(() => removeToast(id), duration);
    timersRef.current.set(id, timer);
  }, [duration, removeToast]);

  // Capture the ref into a local so the cleanup closure doesn't read
  // a possibly-replaced ref at unmount time.
  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      for (const t of timers.values()) clearTimeout(t);
      timers.clear();
    };
  }, []);

  return { toasts, addToast, removeToast };
}

// ─── Toast Container Component ───────────────────────────────────────────────

const LEVEL_ICON: Record<ToastLevel, typeof Info> = {
  success: CheckCircle,
  error: XCircle,
  warning: AlertTriangle,
  info: Info,
};

const LEVEL_CLASS: Record<ToastLevel, string> = {
  success: "toast-success",
  error: "toast-error",
  warning: "toast-warning",
  info: "toast-info",
};

interface ToastContainerProps {
  toasts: Toast[];
  onDismiss: (id: number) => void;
}

export function ToastContainer({ toasts, onDismiss }: ToastContainerProps) {
  if (toasts.length === 0) return null;

  return (
    <div className="toast-container">
      {toasts.map((toast) => {
        const Icon = LEVEL_ICON[toast.level];
        return (
          <div key={toast.id} className={`toast ${LEVEL_CLASS[toast.level]} ${toast.exiting ? "toast-exit" : "toast-enter"}`}>
            <Icon size={16} className="toast-level-icon" />
            <span className="toast-message">{toast.message}</span>
            <button className="toast-dismiss" onClick={() => onDismiss(toast.id)}>
              <X size={14} />
            </button>
          </div>
        );
      })}
    </div>
  );
}

