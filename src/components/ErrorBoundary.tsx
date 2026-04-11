import { Component, type ReactNode } from "react";

interface ErrorBoundaryProps {
  children: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("Uncaught error:", error, info.componentStack);
  }

  handleReload = () => {
    window.location.reload();
  };

  render() {
    if (this.state.hasError) {
      return (
        <div style={{
          display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
          height: "100vh", padding: "2rem", textAlign: "center",
          background: "var(--bg-0, #0d1117)", color: "var(--text-1, #c9d1d9)",
          fontFamily: '"Inter", "SF Pro Display", -apple-system, sans-serif',
        }}>
          <div style={{ fontSize: 48, marginBottom: 16 }}>⚠️</div>
          <h1 style={{ fontSize: 22, fontWeight: 600, margin: "0 0 8px", color: "var(--text-0, #f0f6fc)" }}>
            Something went wrong
          </h1>
          <p style={{ fontSize: 14, color: "var(--text-2, #8b949e)", maxWidth: 480, marginBottom: 16 }}>
            The application encountered an unexpected error. You can try reloading to recover.
          </p>
          {this.state.error && (
            <pre style={{
              fontSize: 12, padding: "12px 16px", borderRadius: 8,
              background: "var(--bg-2, #21262d)", color: "var(--red, #f85149)",
              maxWidth: 600, overflow: "auto", whiteSpace: "pre-wrap", wordBreak: "break-word",
              marginBottom: 20, textAlign: "left",
            }}>
              {this.state.error.message}
            </pre>
          )}
          <button
            onClick={this.handleReload}
            style={{
              padding: "8px 20px", fontSize: 14, fontWeight: 500, borderRadius: 8,
              border: "none", cursor: "pointer",
              background: "var(--green, #3ddc84)", color: "#000",
            }}
          >
            Reload Application
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}

