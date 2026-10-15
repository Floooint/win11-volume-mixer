import { Check, Ellipsis, Headphones, Monitor, Speaker } from "lucide-react";
import { type ReactNode, useState } from "react";
import type { OutputDevice, OutputKind, OutputRole } from "@/bindings";
import { Expand } from "@/components/Expand";
import { IconButton } from "@/components/IconButton";
import { Tooltip } from "@/components/Tooltip";
import { menuData } from "@/lib/context-menu";
import { cn } from "@/lib/utils";

const KIND_ICONS: Record<OutputKind, typeof Speaker> = {
  speakers: Speaker,
  headphones: Headphones,
  display: Monitor,
};

export function OutputIcon({ kind, className }: { kind: OutputKind; className?: string }) {
  const Icon = KIND_ICONS[kind];
  return <Icon size={16} aria-hidden className={cn("shrink-0", className)} />;
}

/**
 * 输出设备列表（点击系统音量弹出）。点击设备同时用于播放和通话（系统中的“默认设备”和
 * “默认通信设备”）；“…”中可以只设置其中一项。
 */
export function OutputPicker({
  outputs,
  aliases,
  renaming,
  renameField,
  onSelect,
}: {
  outputs: OutputDevice[];
  /** 设备标识 → 重命名。 */
  aliases: Map<string, string>;
  /** 正在重命名的设备，该行显示 `renameField`。 */
  renaming: string | null;
  renameField: (output: OutputDevice) => ReactNode;
  /** `none` 表示点的是已在使用的设备，只需收起列表。 */
  onSelect: (deviceId: string, role: OutputRole | "none") => void;
}) {
  const [more, setMore] = useState<string | null>(null);
  // 播放和通话不是同一个设备时，标出通话设备。
  const splitRoles = outputs.some((o) => o.isDefault !== o.isCommunications);
  return (
    <ul aria-label="输出设备" className="flex flex-col">
      {outputs.map((output) => {
        const name = aliases.get(output.id) ?? output.name;
        return (
          <li
            key={output.id}
            // 右键：复制设备名、重命名（动作由 MainPage 的 onMenuAction 处理）。
            data-menu={menuData([
              { label: "复制设备名", value: output.name },
              { separator: true },
              { label: "重命名", action: `rename-output:${output.id}` },
              aliases.has(output.id) && { label: "恢复原名", action: `reset-output-name:${output.id}` },
            ])}
            className="relative"
          >
            {renaming === output.id && renameField(output)}
            <div className="flex items-center gap-1 rounded-md hover:bg-accent">
              <button
                type="button"
                // 已同时用于播放和通话时不再设置，只收起列表（避免系统重建一次设备）。
                onClick={() =>
                  onSelect(
                    output.id,
                    output.isDefault && output.isCommunications ? "none" : "all",
                  )
                }
                className={cn(
                  "flex min-w-0 flex-1 items-center gap-2 rounded-md py-1.5 pl-2 text-left text-sm",
                  "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
                  output.isDefault ? "font-medium text-foreground" : "text-muted-foreground",
                )}
              >
                <OutputIcon kind={output.kind} className={output.isDefault ? "text-primary" : ""} />
                <Tooltip content={name} onlyWhenTruncated>
                  <span className="truncate">{name}</span>
                </Tooltip>
                {splitRoles && output.isCommunications && (
                  <span className="shrink-0 rounded bg-muted px-1 text-[11px] text-muted-foreground">
                    通话
                  </span>
                )}
                {output.isDefault && (
                  <Check size={14} aria-label="当前设备" className="ml-auto shrink-0 text-primary" />
                )}
              </button>
              <IconButton
                label="更多选项"
                aria-expanded={more === output.id}
                onClick={() => setMore(more === output.id ? null : output.id)}
                className="size-7"
              >
                <Ellipsis size={16} />
              </IconButton>
            </div>
            <Expand open={more === output.id} fitWindow={false}>
              <div className="flex flex-wrap gap-1 py-1 pl-8">
                <RoleButton
                  label="仅用于播放"
                  hint="音乐、视频、游戏等大多数声音。即系统设置中的“默认设备”"
                  done={output.isDefault}
                  onClick={() => onSelect(output.id, "default")}
                />
                <RoleButton
                  label="仅用于通话"
                  hint="QQ、微信、Teams 等的语音通话。即系统设置中的“默认通信设备”"
                  done={output.isCommunications}
                  onClick={() => onSelect(output.id, "communications")}
                />
              </div>
            </Expand>
          </li>
        );
      })}
    </ul>
  );
}

function RoleButton({
  label,
  hint,
  done,
  onClick,
}: {
  label: string;
  hint: string;
  done: boolean;
  onClick: () => void;
}) {
  // 已是当前设置时用 aria-disabled 而不是 disabled：禁用的按钮收不到悬停，看不到说明。
  return (
    <Tooltip content={hint}>
      <button
        type="button"
        aria-disabled={done}
        onClick={() => !done && onClick()}
        className={cn(
          "rounded-md border border-border px-2 py-0.5 text-xs",
          "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
          done ? "cursor-default opacity-50" : "hover:bg-accent",
        )}
      >
        {label}
      </button>
    </Tooltip>
  );
}
