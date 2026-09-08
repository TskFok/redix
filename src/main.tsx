import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles.css";

// Capture at the document level so dialogs and portals are covered too.
document.addEventListener("contextmenu", (event) => event.preventDefault(), { capture: true });

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
