// eslint-disable-next-line @typescript-eslint/triple-slash-reference
/// <reference path="./renderer.d.ts" />

import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { QuestionPrompt } from "./components/QuestionPrompt";
import { ThemeProvider } from "./renderer/Theme";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider>
      <App />
      <QuestionPrompt />
    </ThemeProvider>
  </React.StrictMode>,
);
