import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { commands } from "./bindings";
import { installContextCopy } from "./lib/context-copy";
import "./index.css";

installContextCopy();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

// 等浏览器完成首帧绘制后再通知后端显示窗口，避免出现空白窗口。
requestAnimationFrame(() => requestAnimationFrame(() => void commands.windowReady()));
