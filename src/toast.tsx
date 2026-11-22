import React from "react";
import ReactDOM from "react-dom/client";
import ConnectionToast from "./components/ConnectionToast";
import "./styles/theme.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ConnectionToast />
  </React.StrictMode>,
);
