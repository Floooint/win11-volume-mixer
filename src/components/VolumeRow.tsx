import { type ReactNode, type RefObject, useRef } from "react";
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
  /** 一次音量调节结束时调用（松开拖动、滚轮停止）。 */
  onVolumeCommit?: () => void;
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
  const percent = Math.round(volume.volume * 100);
  const inactive = active === false;

  return (
    <div ref={rowRef} className={cn("flex items-center gap-3", className)}>
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
            <span className="text-xs tabular-nums text-muted-foreground">
              {volume.muted ? "静音" : `${percent}%`}
            </span>
            {trailing}
          </div>
        </div>
        {detail && <p className="truncate text-xs text-muted-foreground">{detail}</p>}
        <div
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
