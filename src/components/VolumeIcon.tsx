import { Volume1 } from "@/components/animate-ui/icons/volume-1";
import { Volume2 } from "@/components/animate-ui/icons/volume-2";
import { VolumeOff } from "@/components/animate-ui/icons/volume-off";

/** 按音量大小切换图标；须放在带 `AnimateIcon` 的父元素内才会在悬停时播放动画。 */
export function VolumeIcon({ volume, muted, size = 16 }: { volume: number; muted: boolean; size?: number }) {
  if (muted || volume === 0) return <VolumeOff size={size} />;
  if (volume < 0.5) return <Volume1 size={size} />;
  return <Volume2 size={size} />;
}
