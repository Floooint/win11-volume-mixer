import { cn } from "@/lib/utils";

/** 加载占位块，缓慢明暗闪烁。尺寸和圆角由调用方指定。 */
export function Skeleton({ className }: { className?: string }) {
  return (
    <div
      aria-hidden
      className={cn("animate-pulse rounded-md bg-muted-foreground/15", className)}
    />
  );
}

/** 一行音量控制的占位，尺寸与 `VolumeRow` 一致：名称行、说明行（可选）、静音按钮和滑块。 */
function VolumeRowSkeleton({ avatar, detail }: { avatar?: boolean; detail?: boolean }) {
  return (
    <div className="flex items-center gap-3">
      {avatar && <Skeleton className="size-8 shrink-0 rounded-lg" />}
      <div className="min-w-0 flex-1">
        <div className="flex h-5 items-center justify-between">
          <Skeleton className="h-3.5 w-24" />
          <Skeleton className="h-3 w-8" />
        </div>
        {detail && (
          <div className="flex h-4 items-center">
            <Skeleton className="h-3 w-40" />
          </div>
        )}
        <div className="mt-1 flex h-7 items-center gap-2.5">
          <Skeleton className="size-4 shrink-0 rounded-full" />
          <Skeleton className="h-1 flex-1 rounded-full" />
        </div>
      </div>
    </div>
  );
}

/**
 * 首次读取音频状态时的主页面占位：系统音量卡片和几行应用，卡片位置随“系统音量置底”设置。
 * 放在右侧预留了滚动条的滚动区中。
 */
export function MainPageSkeleton({ masterAtBottom }: { masterAtBottom: boolean }) {
  const master = (
    <div className="mr-1 ml-3 rounded-xl border border-border bg-card px-3 py-3">
      <VolumeRowSkeleton detail />
    </div>
  );
  return (
    <div role="status" aria-label="正在读取音频设备">
      {!masterAtBottom && master}
      <Skeleton className={cn("mb-2 ml-4 h-3 w-8", masterAtBottom ? "mt-2" : "mt-5")} />
      <ul className="flex flex-col gap-1 pr-1 pl-3">
        {[0, 1, 2].map((i) => (
          <li key={i} className="px-2 py-2">
            <VolumeRowSkeleton avatar />
          </li>
        ))}
      </ul>
      {masterAtBottom && <div className="mt-2">{master}</div>}
    </div>
  );
}
