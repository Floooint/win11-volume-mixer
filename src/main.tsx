import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { commands } from "./bindings";
import { DetailsView } from "./components/DetailsView";
import { installContextMenu } from "./lib/context-menu";
import "./index.css";

// 同一份页面同时用于主窗口和应用详情浮窗（见 src-tauri/src/details.rs）。
const isDetails = getCurrentWindow().label === "details";

installContextMenu();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isDetails ? (
      // 浮窗不经过 App，单独提供 Tooltip 的 Provider（路径链接的提示）。
      <TooltipPrimitive.Provider delayDuration={300}>
        <DetailsView />
      </TooltipPrimitive.Provider>
    ) : (
      <App />
    )}
  </React.StrictMode>,
);

// 等浏览器完成首帧绘制后再通知后端显示窗口，避免出现空白窗口。浮窗由它自己报告尺寸后显示。
if (!isDetails) {
  requestAnimationFrame(() => requestAnimationFrame(() => void commands.windowReady()));
}
