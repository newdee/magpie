import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// browser demo mode: install the IPC mock before the app makes any calls
if (import.meta.env.VITE_DEMO) {
  await import("./demo-mock");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
