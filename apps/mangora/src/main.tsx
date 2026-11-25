import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { MangoraAppearanceProvider } from "./features/hito5";
import "./styles/editorial.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MangoraAppearanceProvider>
      <App />
    </MangoraAppearanceProvider>
  </StrictMode>,
);
