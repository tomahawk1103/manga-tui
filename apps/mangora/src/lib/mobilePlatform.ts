import { useEffect } from "react";

interface WakeLockSentinelLike {
  released?: boolean;
  release: () => Promise<void>;
}

interface NavigatorWithWakeLock {
  wakeLock?: {
    request: (type: "screen") => Promise<WakeLockSentinelLike>;
  };
}

/**
 * Keeps the device screen awake while reading or downloading on mobile devices.
 * Safely degrades to a no-op if the WebView/browser does not expose Screen Wake Lock.
 */
export function useScreenWakeLock(active: boolean): void {
  useEffect(() => {
    if (!active || typeof navigator === "undefined" || typeof document === "undefined") {
      return;
    }

    const nav = navigator as Navigator & NavigatorWithWakeLock;
    if (!nav.wakeLock || typeof nav.wakeLock.request !== "function") {
      return;
    }

    let cancelled = false;
    let sentinel: WakeLockSentinelLike | null = null;

    const acquire = async () => {
      if (cancelled || document.visibilityState !== "visible") return;
      try {
        const next = await nav.wakeLock?.request("screen");
        if (!next) return;
        if (cancelled) {
          await next.release().catch(() => {});
          return;
        }
        sentinel = next;
      } catch {
        // Ignore platform restrictions (e.g. low battery mode or unsupported WebView)
      }
    };

    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible" && (!sentinel || sentinel.released)) {
        void acquire();
      }
    };

    void acquire();
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      cancelled = true;
      document.removeEventListener("visibilitychange", handleVisibilityChange);
      if (sentinel) {
        void sentinel.release().catch(() => {});
        sentinel = null;
      }
    };
  }, [active]);
}

export async function shareOrDownloadBackupJson(
  jsonContent: string,
  fileName = "library_sync.json",
): Promise<"shared" | "downloaded"> {
  const blob = new Blob([jsonContent], { type: "application/json;charset=utf-8" });

  if (typeof navigator !== "undefined" && typeof File !== "undefined") {
    const file = new File([blob], fileName, { type: "application/json" });
    const canShareFiles =
      typeof navigator.canShare === "function" &&
      typeof navigator.share === "function" &&
      navigator.canShare({ files: [file] });

    if (canShareFiles) {
      try {
        await navigator.share({
          files: [file],
          title: fileName,
        });
        return "shared";
      } catch (err) {
        if (err instanceof DOMException && err.name === "AbortError") {
          return "shared";
        }
      }
    }
  }

  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.style.display = "none";
  document.body.appendChild(anchor);
  anchor.click();
  document.body.removeChild(anchor);
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  return "downloaded";
}
