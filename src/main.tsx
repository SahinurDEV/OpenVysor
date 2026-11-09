import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { PhoneWindow } from "@/windows/PhoneWindow";
import { applyTheme, useSettingsStore } from "@/stores/settingsStore";
import "./index.css";

applyTheme(useSettingsStore.getState().theme);

const phone = window.__OPENVYSOR_PHONE__;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {phone ? <PhoneWindow info={phone} /> : <App />}
  </React.StrictMode>,
);
