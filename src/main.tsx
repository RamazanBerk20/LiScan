import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import * as Tooltip from "@radix-ui/react-tooltip";
import { App } from "./App";
import "./styles.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Tooltip.Provider delayDuration={350}>
      <App />
    </Tooltip.Provider>
  </StrictMode>
);
