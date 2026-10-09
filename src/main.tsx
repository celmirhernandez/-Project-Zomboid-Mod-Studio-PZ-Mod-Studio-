import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components/common/ErrorBoundary";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {/* Outermost boundary: catches a throw during App module evaluation or a
        failure in the shared chrome (header / sidebar) so the user always gets a
        recovery screen instead of a blank window. */}
    <ErrorBoundary name="PZ Mod Studio">
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
