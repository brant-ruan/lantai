import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { DatabaseGate } from "./components/DatabaseGate";
import { LanguageProvider } from "./i18n";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <DatabaseGate>
      <LanguageProvider>
        <App />
      </LanguageProvider>
    </DatabaseGate>
  </React.StrictMode>,
);
