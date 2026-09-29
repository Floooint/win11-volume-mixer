import { type ReactNode, type RefObject, useRef, useState } from "react";
import type { VolumeState } from "@/bindings";
import { AudioLines } from "@/components/animate-ui/icons/audio-lines";
import { IconButton } from "@/components/IconButton";
import { VolumeIcon } from "@/components/VolumeIcon";
import { VolumeSlider } from "@/components/VolumeSlider";
import { cn } from "@/lib/utils";

type VolumeRowProps = {
  name: string;
  /** 名称下方的补充说明，如设备名或“2 个会话”。 */
  detail?: string;
  volume: VolumeState;
  onVolumeChange: (volume: number) => void;
  onMuteChange: (muted: boolean) => void;
  /** 一次音量调节结束时调用（松开拖动、滚轮停止、输入音量），参数为最终音量。 */
  onVolumeCommit?: (volume: number) => void;
  /** 是否正在发声：高亮显示并出现“播放中”动画。`undefined` 表示不区分（如系统音量）。 */
  active?: boolean;
  /** 左侧的应用图标或占位字母。 */
  leading?: ReactNode;
  /** 名称右侧的额外按钮，如调试用的删除按钮。 */
  trailing?: ReactNode;
  /** 所在的滚动列表，见 `VolumeSlider` 的同名参数。 */
  scrollAreaRef?: RefObject<HTMLElement | null>;
  className?: string;
};

/**
 * 音量数字：平时隐藏，鼠标悬停在这一行（或键盘焦点在行内）时显示；点击后变为输入框，
 * 回车或失焦确认，Esc 取消，↑ / ↓ 每次调 1。静音时显示“静音”（始终可见），点击同样可输入音量。
 */
function VolumeValue({
  name,
  volume,
  onChange,
  onCommit,
}: {
  name: string;
  volume: VolumeState;
  onChange: (volume: number) => void;
  onCommit?: (volume: number) => void;
}) {
  const percent = Math.round(volume.volume * 100);
  const [draft, setDraft] = useState<string | null>(null);
  // Esc 后让输入框失焦，由失焦统一结束；此标记让失焦时按“取消”处理。
  const cancelled = useRef(false);

  const finish = () => {
    const value = Number.parseInt(draft ?? "", 10);
    setDraft(null);
    if (cancelled.current || Number.isNaN(value)) return;
    const volume = Math.min(100, Math.max(0, value)) / 100;
    onChange(volume);
    onCommit?.(volume);
  };

  if (draft !== null) {
    return (
      <input
        autoFocus
        aria-label={`${name} 音量（0–100）`}
        inputMode="numeric"
        maxLength={3}
        value={draft}
        onChange={(e) => setDraft(e.target.value.replace(/\D/g, ""))}
        onFocus={(e) => e.currentTarget.select()}
        onBlur={finish}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
          if (e.key === "Escape") {
            cancelled.current = true;
            e.currentTarget.blur();
          }
          if (e.key === "ArrowUp" || e.key === "ArrowDown") {
            e.preventDefault();
            const current = Number.parseInt(draft, 10) || 0;
            const next = current + (e.key === "ArrowUp" ? 1 : -1);
            setDraft(String(Math.min(100, Math.max(0, next))));
          }
        }}
        className={cn(
          "h-5 w-10 rounded border border-primary/60 bg-background px-1 text-right text-xs tabular-nums",
          "text-foreground outline-none select-text focus-visible:ring-2 focus-visible:ring-ring",
        )}
      />
    );
  }
  return (
    <button
      type="button"
      title="点击输入音量"
      aria-label={volume.muted ? `${name} 已静音，点击输入音量` : `${name} 音量 ${percent}%，点击输入`}
      onClick={() => {
        cancelled.current = false;
        setDraft(String(percent));
      }}
      className={cn(
        "rounded px-1 text-xs tabular-nums text-muted-foreground transition-opacity",
        "hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
        // 静音提示始终可见；数字只在悬停或焦点在行内时显示（仍占位，显示时不跳动）。
        !volume.muted &&
          "opacity-0 group-hover/row:opacity-100 group-focus-within/row:opacity-100",
      )}
    >
      {volume.muted ? "静音" : `${percent}%`}
    </button>
  );
}

/**
 * 一行音量控制：名称、静音按钮、滑块、百分比。系统音量和应用音量共用。
 * 鼠标在整行任意位置滚动滚轮即可调节音量（所在列表可滚动时除外）。
 */
export function VolumeRow({
  name,
  detail,
  volume,
  onVolumeChange,
  onMuteChange,
  onVolumeCommit,
  active,
  leading,
  trailing,
  scrollAreaRef,
  className,
}: VolumeRowProps) {
  const rowRef = useRef<HTMLDivElement>(null);
  const inactive = active === false;

  return (
    <div ref={rowRef} className={cn("group/row flex items-center gap-3", className)}>
      {leading && (
        <div className={cn("transition-opacity", inactive && "opacity-60")}>{leading}</div>
      )}
      <div className="min-w-0 flex-1">
        <div className="flex items-center justify-between gap-2">
          <div className="flex min-w-0 items-center gap-1.5">
            {active && !volume.muted && (
              <AudioLines
                size={14}
                animate
                loop
                aria-label="正在播放"
                className="shrink-0 text-primary"
              />
            )}
            <p
              className={cn(
                "truncate text-sm transition-colors",
                active ? "font-semibold text-foreground" : "font-medium",
                inactive && "text-muted-foreground",
              )}
              title={name}
            >
              {name}
            </p>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <VolumeValue
              name={name}
              volume={volume}
              onChange={onVolumeChange}
              onCommit={onVolumeCommit}
            />
            {trailing}
          </div>
        </div>
        {detail && <p className="truncate text-xs text-muted-foreground">{detail}</p>}
        {/* 静音按钮和滑块一行用于调节音量：点在这里（包括轨道、按钮之间的空隙）不切换应用详情。 */}
        <div
          data-no-details
          className={cn(
            "mt-1 flex items-center gap-1 transition-opacity",
            inactive && "opacity-60",
          )}
        >
          <IconButton
            label={volume.muted ? `取消静音 ${name}` : `静音 ${name}`}
            onClick={() => onMuteChange(!volume.muted)}
            className="-ml-1.5 size-7"
          >
            <VolumeIcon volume={volume.volume} muted={volume.muted} />
          </IconButton>
          <VolumeSlider
            label={`${name} 音量`}
            value={volume.volume}
            muted={volume.muted}
            onChange={onVolumeChange}
            onCommit={onVolumeCommit}
            wheelAreaRef={rowRef}
            scrollAreaRef={scrollAreaRef}
          />
        </div>
      </div>
    </div>
  );
}
