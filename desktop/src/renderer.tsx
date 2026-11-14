// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="./renderer.d.ts" />

import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { QuestionPrompt } from "./components/QuestionPrompt";
import { ThemeProvider } from "./renderer/Theme";
import "./index.css";
import "./design.css";

class RendererErrorBoundary extends React.Component<React.PropsWithChildren, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    console.error("[luma] Renderer crashed:", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return <main className="luma-fatal-error" role="alert">
        <h1>Luma ran into a problem</h1>
        <p>{this.state.error.message || "An unexpected interface error occurred."}</p>
        <button type="button" onClick={() => window.location.reload()}>Reload Luma</button>
      </main>;
    }
    return this.props.children;
  }
}

const root = document.getElementById("root");
if (!root) throw new Error("Luma renderer root element is missing.");

// Platform-specific titlebar geometry must be active before the first paint,
// especially while the sidebar is collapsed beneath macOS traffic lights.
document.documentElement.dataset.platform = window.luma.platform;

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <RendererErrorBoundary>
      <ThemeProvider>
        <App />
        <QuestionPrompt />
      </ThemeProvider>
    </RendererErrorBoundary>
  </React.StrictMode>,
);
