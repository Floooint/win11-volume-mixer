import { VolumeIcon } from "@/components/VolumeIcon";
import { IconButton } from "@/components/IconButton";
import { VolumeSlider } from "@/components/VolumeSlider";
import type { VolumeState } from "@/bindings";
import { cn } from "@/lib/utils";

type VolumeRowProps = {
  name: string;
  /** 名称下方的补充说明，如设备名或“2 个会话”。 */
  detail?: string;
  volume: VolumeState;
  onVolumeChange: (volume: number) => void;
  onMuteChange: (muted: boolean) => void;
  /** 应用未在播放时整行弱化。 */
  dimmed?: boolean;
  /** 左侧的应用图标或占位字母。 */
  leading?: React.ReactNode;
  className?: string;
};

/** 一行音量控制：名称、静音按钮、滑块、百分比。系统音量和应用音量共用。 */
export function VolumeRow({
  name,
  detail,
  volume,
  onVolumeChange,
  onMuteChange,
  dimmed,
  leading,
  className,
}: VolumeRowProps) {
  const percent = Math.round(volume.volume * 100);
  return (
    <div className={cn("flex items-center gap-3", dimmed && "opacity-70", className)}>
      {leading}
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline justify-between gap-2">
          <p className="truncate text-sm font-medium" title={name}>
            {name}
          </p>
          <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
            {volume.muted ? "静音" : `${percent}%`}
          </span>
        </div>
        {detail && <p className="truncate text-xs text-muted-foreground">{detail}</p>}
        <div className="mt-1 flex items-center gap-1">
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
          />
        </div>
      </div>
    </div>
  );
}
