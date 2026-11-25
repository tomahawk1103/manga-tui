import { Component, StrictMode, type ErrorInfo, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { MangoraAppearanceProvider } from "./features/hito5";
import "./styles/editorial.css";

interface ErrorBoundaryState {
  error: Error | null;
}

class AppErrorBoundary extends Component<{ children: ReactNode }, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Mangora render error:", error, info);
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex min-h-dvh items-center justify-center bg-[var(--paper)] p-6 text-[var(--ink)]">
          <div className="editorial-card max-w-md space-y-4 p-6">
            <div className="flex items-center gap-3">
              <span className="mark" aria-hidden="true">
                M!
              </span>
              <h1 className="font-display text-2xl">Mangora</h1>
            </div>
            <p className="text-sm font-bold text-[var(--red-text)]">
              Error inesperado al mostrar la interfaz.
            </p>
            <pre className="max-h-48 overflow-auto border border-[var(--ink)] bg-[var(--surface)] p-3 font-mono-editorial text-xs whitespace-pre-wrap">
              {this.state.error.message}
            </pre>
            <button
              type="button"
              onClick={() => window.location.reload()}
              className="editorial-btn-primary text-xs"
            >
              Recargar Mangora
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <AppErrorBoundary>
      <MangoraAppearanceProvider>
        <App />
      </MangoraAppearanceProvider>
    </AppErrorBoundary>
  </StrictMode>,
);
