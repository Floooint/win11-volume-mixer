import { convertFileSrc } from "@tauri-apps/api/core";
import { useState } from "react";
import type { AppAudio } from "@/bindings";
import { Skeleton } from "@/components/Skeleton";
import { cn } from "@/lib/utils";

/**
 * 应用图标，由后端经 `appicon` 协议提供。加载期间显示骨架占位，避免首字母一闪而过；
 * 没有图标来源或加载失败时显示首字母占位，正在发声时换成强调色。
 * 调用方以 `app.icon` 作为 key，来源变化时重置加载状态。
 */
export function AppAvatar({ app }: { app: AppAudio }) {
  const [status, setStatus] = useState<"loading" | "loaded" | "error">(
    app.icon ? "loading" : "error",
  );

  if (app.icon && status !== "error") {
    return (
      <div aria-hidden className="relative flex size-8 shrink-0 items-center justify-center">
        {status === "loading" && <Skeleton className="absolute size-7 rounded-lg" />}
        <img
          src={convertFileSrc(app.icon, "appicon")}
          alt=""
          draggable={false}
          onLoad={() => setStatus("loaded")}
          onError={() => setStatus("error")}
          className={cn(
            "relative size-7 object-contain transition-opacity",
            status === "loading" && "opacity-0",
          )}
        />
      </div>
    );
  }

  const letter = app.appId === "system" ? "系" : (app.name.trim()[0] ?? "?").toUpperCase();
  return (
    <div
      aria-hidden
      className={cn(
        "flex size-8 shrink-0 items-center justify-center rounded-lg text-sm font-semibold transition-colors",
        app.active
          ? "bg-primary/15 text-primary ring-1 ring-primary/30"
          : "bg-secondary text-secondary-foreground",
      )}
    >
      {letter}
    </div>
  );
}
