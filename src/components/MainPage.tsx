import {
  BookmarkPlus,
  ChevronRight,
  FolderClosed,
  FolderPlus,
  GripVertical,
  Pin,
  Plus,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import {
  type ReactNode,
  type Ref,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import {
  type AppAlias,
  type AppAudio,
  type AppGroup,
  commands,
  type PinMode,
  type SavedApp,
  type Scene,
  type SceneApp,
} from "@/bindings";
import { AudioLines } from "@/components/animate-ui/icons/audio-lines";
import { Settings as SettingsIcon } from "@/components/animate-ui/icons/settings";
import { Trash2 } from "@/components/animate-ui/icons/trash-2";
import { VolumeOff } from "@/components/animate-ui/icons/volume-off";
import { AppAvatar } from "@/components/AppAvatar";
import { IconButton } from "@/components/IconButton";
import { MainPageSkeleton } from "@/components/Skeleton";
import { VolumeRow } from "@/components/VolumeRow";
import { type DragState, type DropTarget, sameTarget, useAppDrag } from "@/hooks/use-app-drag";
import { announceHeightAnimation, useFitWindowHeight } from "@/hooks/use-fit-window-height";
import { type MenuItem, menuData, onMenuAction } from "@/lib/context-menu";
import { closeAppDetails, closeAppDetailsOnPointerDown, toggleAppDetails } from "@/lib/app-details";
import { cn } from "@/lib/utils";
import { useAudioStore } from "@/stores/audio";
import { useSettingsStore } from "@/stores/settings";
import { DEBUG_APP_PREFIX, useDebugStore } from "@/stores/debug";

const PIN_MODES: Record<PinMode, { next: PinMode; label: string }> = {
  normal: { next: "pinned", label: "固定窗口：当前失焦自动隐藏，点击改为定住" },
  pinned: { next: "pinnedOnTop", label: "固定窗口：当前已定住，点击改为定住并置顶" },
  pinnedOnTop: { next: "normal", label: "固定窗口：当前已定住并置顶，点击恢复失焦隐藏" },
};

/**
 * 图钉：在“正常（失焦隐藏）”“定住”“定住并置顶”之间循环切换。
 * 状态保存在后端（只在本次运行中有效），窗口重建后重新读取。
 */
function PinButton() {
  const [mode, setMode] = useState<PinMode>("normal");
  useEffect(() => {
    commands.getPinMode().then(setMode);
  }, []);

  const { next, label } = PIN_MODES[mode];
  return (
    <IconButton
      label={label}
      aria-pressed={mode !== "normal"}
      onClick={() => {
        setMode(next);
        void commands.setPinMode(next);
      }}
      className={cn(
        mode !== "normal" && "text-primary hover:text-primary",
        mode === "pinnedOnTop" && "bg-primary/15 hover:bg-primary/20",
      )}
    >
      <Pin
        size={16}
        fill={mode === "pinnedOnTop" ? "currentColor" : "none"}
        className={cn("transition-transform duration-150", mode === "normal" && "rotate-45")}
      />
    </IconButton>
  );
}

/** 列表区域的空状态或错误状态。 */
function EmptyState({
  icon,
  title,
  hint,
  action,
}: {
  icon: ReactNode;
  title: string;
  hint?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-1 px-6 py-10 text-center text-sm text-muted-foreground">
      <div className="mb-1">{icon}</div>
      <p className="text-foreground">{title}</p>
      {hint && <p className="text-xs">{hint}</p>}
      {action}
    </div>
  );
}

/**
 * 首次运行时询问是否开机自启。选择后写入系统注册并不再询问；
 * 之后可在设置页修改。
 */
function AutostartPrompt() {
  const save = useSettingsStore((s) => s.save);
  const [error, setError] = useState<string | null>(null);
  const answer = async (enabled: boolean) => {
    if (enabled) {
      const result = await commands.setAutostart(true);
      if (result.status === "error") {
        setError(result.error.message);
        return;
      }
    }
    await save({ autostartPrompt: false });
  };
  return (
    <section
      aria-label="开机自启"
      className="mx-3 mb-2 rounded-xl border border-primary/40 bg-primary/7 px-3 py-2.5"
    >
      <p className="font-medium">是否开启开机自启动？</p>
      <p className="mt-0.5 text-xs text-muted-foreground">
        登录 Windows 后自动在托盘运行，不弹出窗口。之后可在设置中修改。
      </p>
      {error && <p className="mt-1 text-xs text-destructive">{error}</p>}
      <div className="mt-2 flex justify-end gap-2">
        <button
          type="button"
          onClick={() => void answer(false)}
          className="rounded-md border border-border bg-card px-3 py-1 text-xs hover:bg-accent"
        >
          暂不开启
        </button>
        <button
          type="button"
          onClick={() => void answer(true)}
          className="rounded-md bg-primary px-3 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90"
        >
          开启
        </button>
      </div>
    </section>
  );
}

function ErrorToast() {
  const error = useAudioStore((s) => s.error);
  const clearError = useAudioStore((s) => s.clearError);

  useEffect(() => {
    if (!error) return;
    const timer = setTimeout(clearError, 4000);
    return () => clearTimeout(timer);
  }, [error, clearError]);

  if (!error) return null;
  return (
    <div
      role="alert"
      className="absolute inset-x-3 bottom-3 rounded-lg border border-destructive/30 bg-card px-3 py-2 text-xs text-destructive shadow-lg"
    >
      {error}
    </div>
  );
}

const NO_APPS: SavedApp[] = [];
const NO_GROUPS: AppGroup[] = [];
const NO_ALIASES: AppAlias[] = [];
const NO_SCENES: Scene[] = [];

/** 列表动画的时长与曲线，与窗口高度动画（animation.rs 的 RESIZE）一致。 */
const LIST_TRANSITION = { duration: 0.18, ease: [0.33, 1, 0.68, 1] } as const;

/** 能否置顶 / 隐藏 / 分组：以 PID 标识的应用重启后标识会变，调试占位应用不写入设置。 */
function canRemember(appId: string) {
  return !appId.startsWith("pid:") && !appId.startsWith(DEBUG_APP_PREFIX);
}

/** 列表中一个应用所在的位置，决定右键菜单和拖动手柄。 */
type Placement = { kind: "pinned" } | { kind: "group"; group: AppGroup } | { kind: "list" };

/**
 * 一个应用。出现时淡入，退出时原地淡出（`popLayout` 下立即让出位置，列表高度只变化一次，
 * 由窗口高度动画过渡），位置变化时平滑移动。`ref` 由 `AnimatePresence` 使用。
 * 悬停时左侧出现拖动手柄，可拖到置顶区、分组或普通区域（只从手柄开始拖动，不影响音量滑块）。
 */
function AppItem({
  app,
  alias,
  renaming,
  onRenameDone,
  placement,
  groups,
  scrollAreaRef,
  dragging,
  detailsDisabled,
  onDragStart,
  onVolumeCommit,
  ref,
}: {
  app: AppAudio;
  /** 用户起的名称，优先于应用名显示。 */
  alias: string | undefined;
  /** 正在重命名：名称处显示输入框。 */
  renaming: boolean;
  /** 重命名结束：`name` 为新名称（空字符串恢复原名），取消时为 `null`。 */
  onRenameDone: (name: string | null) => void;
  placement: Placement;
  groups: AppGroup[];
  scrollAreaRef: RefObject<HTMLElement | null>;
  dragging: boolean;
  /** 拖动或重命名期间点击不显示详情。 */
  detailsDisabled: boolean;
  onDragStart: (e: React.PointerEvent) => void;
  /** 分组内的应用单独调节结束时，记下它在组音量 100% 时的音量（调节期间不写设置文件）。 */
  onVolumeCommit?: (volume: number) => void;
  ref?: Ref<HTMLLIElement>;
}) {
  const setAppVolume = useAudioStore((s) => s.setAppVolume);
  const setAppMute = useAudioStore((s) => s.setAppMute);
  const debug = useDebugStore();
  const isDebug = app.appId.startsWith(DEBUG_APP_PREFIX);
  const remember = canRemember(app.appId);
  const inGroup = placement.kind === "group";
  const name = alias ?? app.name;

  const groupMenu: MenuItem = {
    label: inGroup ? "移到分组" : "添加到分组",
    children: [
      ...groups
        .filter((g) => !(inGroup && g.id === placement.group.id))
        .map((g): MenuItem => ({ label: g.name, action: `group:${g.id}:${app.appId}` })),
      { separator: true },
      { label: "新建分组", action: `new-group:${app.appId}` },
    ],
  };

  return (
    <motion.li
      ref={ref}
      // 只动画位置：用 transform 实现，不改变测得的内容高度。
      layout="position"
      initial={{ opacity: 0, scale: 0.97 }}
      animate={{ opacity: dragging ? 0.4 : 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.97 }}
      transition={LIST_TRANSITION}
      // 点击显示 / 隐藏详情（滑块、按钮等控件除外）。
      data-details-row
      onClick={(e) => !detailsDisabled && toggleAppDetails(e, { app, alias: alias ?? null })}
      data-menu={menuData([
        { label: "复制应用名", value: name },
        alias && { label: "复制原名", value: app.name },
        app.processName && { label: "复制进程名", value: app.processName },
        app.exePath && { label: "复制路径", value: app.exePath },
        remember && { separator: true },
        remember &&
          !inGroup &&
          (placement.kind === "pinned"
            ? { label: "取消置顶", action: `unpin:${app.appId}` }
            : { label: "置顶", action: `pin:${app.appId}` }),
        remember && groupMenu,
        remember && inGroup && { label: "移出分组", action: `ungroup:${app.appId}` },
        remember && { label: "隐藏", action: `hide:${app.appId}` },
        remember && { separator: true },
        remember && { label: "重命名", action: `rename-app:${app.appId}` },
        remember && alias && { label: "恢复原名", action: `reset-name:${app.appId}` },
      ])}
      className={cn(
        "group/app relative rounded-lg px-2 py-2 transition-colors",
        // 正在发声：浅色底，一眼就能找到。
        app.active ? "bg-primary/7 hover:bg-primary/11" : "hover:bg-accent/60",
      )}
    >
      {remember && (
        <span
          aria-hidden
          title="拖动到置顶区或分组"
          data-no-details
          onPointerDown={onDragStart}
          className={cn(
            "absolute top-1/2 left-0 flex h-8 w-2.5 -translate-y-1/2 cursor-grab touch-none items-center justify-center",
            "text-muted-foreground/60 opacity-0 transition-opacity group-hover/app:opacity-100",
          )}
        >
          <GripVertical size={12} />
        </span>
      )}
      {renaming && (
        <RenameField
          label="应用名称"
          initial={name}
          placeholder={app.name}
          onDone={onRenameDone}
          className="absolute top-1.5 right-1 left-13 z-20"
        />
      )}
      <VolumeRow
        name={name}
        detail={
          [app.sessionCount > 1 ? `${app.sessionCount} 个会话` : null, isDebug ? "调试占位" : null]
            .filter(Boolean)
            .join(" · ") || undefined
        }
        volume={app.volume}
        active={app.active}
        leading={
          remember ? (
            // 图标也可以按住拖动，与左侧的拖动手柄相同。
            <span
              onPointerDown={onDragStart}
              title="拖动到置顶区或分组"
              data-no-details
              className="block cursor-grab touch-none active:cursor-grabbing"
            >
              <AppAvatar key={app.icon ?? ""} app={app} />
            </span>
          ) : (
            <AppAvatar key={app.icon ?? ""} app={app} />
          )
        }
        scrollAreaRef={scrollAreaRef}
        trailing={
          <>
            {placement.kind === "pinned" && (
              <Pin size={11} aria-label="已置顶" className="text-muted-foreground/70" />
            )}
            {isDebug && (
              <IconButton
                label={`移除 ${name}`}
                onClick={() => debug.remove(app.appId)}
                className="size-6"
              >
                <Trash2 size={13} />
              </IconButton>
            )}
          </>
        }
        onVolumeChange={(v) => {
          if (isDebug) debug.setVolume(app.appId, v);
          else setAppVolume(app.appId, v);
        }}
        onVolumeCommit={onVolumeCommit}
        onMuteChange={(m) => (isDebug ? debug.setMute(app.appId, m) : setAppMute(app.appId, m))}
      />
    </motion.li>
  );
}

/**
 * 拖动时标出可以放下的位置：平时是浅色虚线框，光标经过时变为强调色，并显示“松手…”提示。
 * 提示显示在框内居中的小标签上，不与下面的文字重叠。
 */
function DropHint({ active, label }: { active: boolean; label?: string }) {
  return (
    <div
      className={cn(
        "pointer-events-none absolute inset-0 z-10 flex items-center justify-center rounded-lg border-2 border-dashed transition-colors",
        active ? "border-primary bg-primary/10" : "border-muted-foreground/25",
      )}
    >
      {active && label && (
        <span className="rounded-md bg-primary px-2 py-0.5 text-xs font-medium text-primary-foreground shadow-sm">
          {label}
        </span>
      )}
    </div>
  );
}

/**
 * 一个分组：标题行（折叠按钮、名称、组音量）和展开后的组内应用。
 * 组音量按比例缩放组内应用：各应用音量 = 它在组音量 100% 时的音量 × 组音量。
 */
function GroupItem({
  group,
  apps,
  groups,
  aliases,
  renaming,
  onRenameDone,
  scrollAreaRef,
  drag,
  onDragStart,
  ref,
}: {
  group: AppGroup;
  /** 组内正在运行的应用。 */
  apps: AppAudio[];
  groups: AppGroup[];
  aliases: Map<string, string>;
  /** 正在重命名的对象：分组本身或组内某个应用。 */
  renaming: Renaming | null;
  onRenameDone: (name: string | null) => void;
  scrollAreaRef: RefObject<HTMLElement | null>;
  drag: DragState | null;
  onDragStart: (app: AppAudio, e: React.PointerEvent) => void;
  ref?: Ref<HTMLLIElement>;
}) {
  const setGroupVolume = useAudioStore((s) => s.setGroupVolume);
  const setGroupMute = useAudioStore((s) => s.setGroupMute);
  const updateGroup = useSettingsStore((s) => s.updateGroup);
  const setMemberVolume = useSettingsStore((s) => s.setMemberVolume);

  const running = group.apps.filter((m) => apps.some((a) => a.appId === m.appId));
  const muted = apps.length > 0 && apps.every((a) => a.volume.muted);
  const active = apps.some((a) => a.active);
  const target: DropTarget = { kind: "group", groupId: group.id };
  const isSource = !!drag && group.apps.some((m) => m.appId === drag.appId);
  // 拖动组滑块时本地记下组音量，松手后才写入设置（设置每次保存都会写文件）。
  const [dragVolume, setDragVolume] = useState<number | null>(null);
  const groupVolume = dragVolume ?? group.volume;
  const membersRef = useRef<HTMLDivElement>(null);

  return (
    <motion.li
      ref={ref}
      layout="position"
      initial={{ opacity: 0, scale: 0.97 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.97 }}
      transition={LIST_TRANSITION}
      data-drop={`group:${group.id}`}
      data-menu={menuData([
        { label: group.expanded ? "折叠" : "展开", action: `toggle-group:${group.id}` },
        { label: "重命名", action: `rename-group:${group.id}` },
        { separator: true },
        { label: "解散分组", action: `delete-group:${group.id}` },
      ])}
      className={cn(
        "relative rounded-lg border border-border/70 px-2 py-2",
        active && "bg-primary/5",
      )}
    >
      {drag && !isSource && (
        <DropHint active={sameTarget(drag.target, target)} label="松手加入分组" />
      )}
      {renaming?.kind === "group" && renaming.id === group.id && (
        <RenameField
          label="分组名称"
          initial={group.name}
          onDone={onRenameDone}
          className="absolute top-1.5 right-1 left-13 z-20"
        />
      )}
      <VolumeRow
        name={group.name}
        detail={
          group.apps.length === 0
            ? "拖动应用到这里，或右键应用“添加到分组”"
            : `${group.apps.length} 个应用${running.length < group.apps.length ? `，${running.length} 个正在运行` : ""}`
        }
        volume={{ volume: groupVolume, muted }}
        active={active}
        leading={
          <button
            type="button"
            aria-label={group.expanded ? `折叠 ${group.name}` : `展开 ${group.name}`}
            aria-expanded={group.expanded}
            onClick={() => void updateGroup(group.id, { expanded: !group.expanded })}
            className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-secondary text-secondary-foreground hover:bg-accent"
          >
            <ChevronRight
              size={16}
              className={cn("transition-transform duration-150", group.expanded && "rotate-90")}
            />
          </button>
        }
        trailing={<FolderClosed size={11} aria-label="分组" className="text-muted-foreground/70" />}
        scrollAreaRef={scrollAreaRef}
        onVolumeChange={(volume) => {
          setDragVolume(volume);
          setGroupVolume(
            running.map((m) => [m.appId, m.fullVolume]),
            volume,
          );
        }}
        onVolumeCommit={(volume) => {
          void updateGroup(group.id, { volume });
          setDragVolume(null);
        }}
        onMuteChange={(m) => setGroupMute(running.map((a) => a.appId), m)}
      />
      {/* 展开 / 折叠：高度从 0 到自然高度，下面的行随之平滑移动，不会与展开的内容重叠。 */}
      <AnimatePresence initial={false}>
        {group.expanded && apps.length > 0 && (
          <motion.div
            key="members"
            ref={membersRef}
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={LIST_TRANSITION}
            onAnimationStart={(target) => {
              // 按动画目标判断方向：折叠时退出动画用的是最后一次渲染的闭包，group.expanded 仍为 true。
              // 从当前高度（中途反向时不为 0 或完整高度）变到 0，或变到完整高度 scrollHeight。
              const el = membersRef.current;
              if (!el) return;
              const collapsing =
                typeof target === "object" && "height" in target && target.height === 0;
              const delta = collapsing ? -el.offsetHeight : el.scrollHeight - el.offsetHeight;
              if (delta !== 0) announceHeightAnimation(delta, LIST_TRANSITION.duration * 1000);
            }}
            className="overflow-hidden"
          >
            <ul className="flex flex-col gap-1 border-l border-border/70 pt-1 pl-2">
              <AnimatePresence initial={false} mode="popLayout">
                {apps.map((app) => (
                  <AppItem
                    key={app.appId}
                    app={app}
                    alias={aliases.get(app.appId)}
                    renaming={renaming?.kind === "app" && renaming.id === app.appId}
                    onRenameDone={onRenameDone}
                    placement={{ kind: "group", group }}
                    groups={groups}
                    scrollAreaRef={scrollAreaRef}
                    dragging={drag?.appId === app.appId}
                    detailsDisabled={!!drag || renaming !== null}
                    onDragStart={(e) => onDragStart(app, e)}
                    onVolumeCommit={(v) => void setMemberVolume(group.id, app.appId, v)}
                  />
                ))}
              </AnimatePresence>
            </ul>
          </motion.div>
        )}
      </AnimatePresence>
    </motion.li>
  );
}

/** 拖动时出现的置顶区。 */
function PinnedDropZone({ drag }: { drag: DragState }) {
  const active = sameTarget(drag.target, { kind: "pinned" });
  return (
    <li
      data-drop="pinned"
      className="relative flex h-10 items-center justify-center gap-1 text-xs text-muted-foreground"
    >
      {!active && (
        <>
          <Pin size={12} />
          拖到这里置顶
        </>
      )}
      <DropHint active={active} label="松手置顶" />
    </li>
  );
}

/** 跟随光标的小标签，显示正在拖动的应用；落在普通区域时提示“松手放回列表”。 */
function DragBadge({ drag }: { drag: DragState }) {
  return (
    <div
      aria-hidden
      style={{ left: drag.x + 12, top: drag.y + 12 }}
      className="pointer-events-none fixed z-50 max-w-48 truncate rounded-md border border-border bg-popover px-2 py-1 text-xs text-popover-foreground shadow-md"
    >
      <span className="font-medium">{drag.name}</span>
      {drag.target?.kind === "list" && drag.from !== "list" && (
        <span className="text-muted-foreground"> · 松手放回列表</span>
      )}
    </div>
  );
}

/** 正在重命名的分组、应用或场景。 */
type Renaming = { kind: "group" | "app" | "scene"; id: string };

/** 场景中正在运行的应用，音量都与场景一致时视为“当前场景”。音量按百分比取整比较。 */
function sceneState(scene: Scene, apps: AppAudio[]): { running: number; current: boolean } {
  let running = 0;
  let current = true;
  for (const saved of scene.apps) {
    const app = apps.find((a) => a.appId === saved.appId);
    if (!app) continue;
    running++;
    const same =
      Math.round(app.volume.volume * 100) === Math.round(saved.volume * 100) &&
      app.volume.muted === saved.muted;
    if (!same) current = false;
  }
  return { running, current: running > 0 && current };
}

/**
 * 场景栏：每个场景一个按钮，点击即把场景中正在运行的应用调到保存的音量。
 * 与当前音量一致的场景高亮；场景中的应用都没在运行时按钮变灰。右键可覆盖、重命名、删除。
 */
function ScenesBar({
  scenes,
  apps,
  aliases,
  renaming,
  onRenameDone,
  onApply,
}: {
  scenes: Scene[];
  apps: AppAudio[];
  aliases: Map<string, string>;
  renaming: Renaming | null;
  onRenameDone: (name: string | null) => void;
  onApply: (scene: Scene) => void;
}) {
  return (
    <div role="toolbar" aria-label="音量场景" className="flex flex-wrap gap-1.5 px-3 pb-2">
      {scenes.map((scene) => {
        if (renaming?.kind === "scene" && renaming.id === scene.id) {
          return (
            <RenameField
              key={scene.id}
              label="场景名称"
              initial={scene.name}
              onDone={onRenameDone}
              className="h-7 w-32 py-0 text-xs"
            />
          );
        }
        const { running, current } = sceneState(scene, apps);
        const summary = scene.apps
          .map((a) => {
            const volume = a.muted ? "静音" : `${Math.round(a.volume * 100)}%`;
            return `${aliases.get(a.appId) ?? a.name}：${volume}`;
          })
          .join("\n");
        return (
          // 不用 disabled：禁用的按钮收不到右键，就无法删除应用都没在运行的场景。
          <button
            key={scene.id}
            type="button"
            aria-pressed={current}
            aria-disabled={running === 0}
            title={running === 0 ? `${summary}\n\n场景中的应用都没在运行` : summary}
            onClick={() => running > 0 && onApply(scene)}
            data-menu={menuData([
              { label: "用当前音量覆盖", action: `scene-overwrite:${scene.id}` },
              { label: "重命名", action: `scene-rename:${scene.id}` },
              { separator: true },
              { label: "删除场景", action: `scene-delete:${scene.id}` },
            ])}
            className={cn(
              "h-7 max-w-40 truncate rounded-full border px-3 text-xs transition-colors",
              "focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
              current
                ? "border-primary bg-primary text-primary-foreground"
                : "border-border bg-card hover:bg-accent",
              running === 0 && "opacity-50",
            )}
          >
            {scene.name}
          </button>
        );
      })}
    </div>
  );
}

/** 与后端 `config::MAX_ALIAS_CHARS` 一致。 */
const MAX_NAME_CHARS = 40;

/**
 * 重命名输入框，覆盖在分组或应用的名称处。回车或失焦完成，Esc 取消。
 * 完成时交出去掉首尾空白的名称（可能为空），取消时交出 `null`。
 */
function RenameField({
  label,
  initial,
  placeholder,
  onDone,
  className,
}: {
  label: string;
  initial: string;
  placeholder?: string;
  onDone: (name: string | null) => void;
  className?: string;
}) {
  const [name, setName] = useState(initial);
  // Esc 后让输入框失焦，由失焦统一结束；此标记让失焦时按“取消”处理。
  const cancelled = useRef(false);
  return (
    <input
      autoFocus
      aria-label={label}
      value={name}
      placeholder={placeholder}
      maxLength={MAX_NAME_CHARS}
      onChange={(e) => setName(e.target.value)}
      onFocus={(e) => e.currentTarget.select()}
      onBlur={() => onDone(cancelled.current ? null : name.trim())}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") {
          cancelled.current = true;
          e.currentTarget.blur();
        }
      }}
      className={cn(
        "rounded-md border border-primary/60 bg-background px-2 py-1 text-sm text-foreground shadow-sm",
        "outline-none focus-visible:ring-2 focus-visible:ring-ring",
        className,
      )}
    />
  );
}

export function MainPage({ onOpenSettings }: { onOpenSettings: () => void }) {
  const snapshot = useAudioStore((s) => s.snapshot);
  const setMasterVolume = useAudioStore((s) => s.setMasterVolume);
  const setMasterMute = useAudioStore((s) => s.setMasterMute);
  const loadError = useAudioStore((s) => s.loadError);
  const retry = useAudioStore((s) => s.retry);
  const debugApps = useDebugStore((s) => s.apps);
  const addDebugApp = useDebugStore((s) => s.add);
  const clearDebugApps = useDebugStore((s) => s.clear);
  const debugTools = useSettingsStore((s) => s.settings?.debugTools ?? false);

  const masterAtBottom = useSettingsStore((s) => s.settings?.masterAtBottom ?? true);
  const appsReversed = useSettingsStore((s) => s.settings?.appsReversed ?? true);
  const autostartPrompt = useSettingsStore((s) => s.settings?.autostartPrompt ?? false);

  const device = snapshot?.device;
  const masterSection = device && (
    <section
      data-menu={menuData([{ label: "复制设备名", value: device.name }])}
      className="mx-3 rounded-xl border border-border bg-card px-3 py-3"
    >
      <VolumeRow
        name="系统音量"
        detail={device.name}
        volume={device.master}
        onVolumeChange={setMasterVolume}
        onMuteChange={setMasterMute}
        onVolumeCommit={() => void commands.playVolumeFeedback()}
      />
    </section>
  );
  const pinnedApps = useSettingsStore((s) => s.settings?.pinnedApps ?? NO_APPS);
  const hiddenApps = useSettingsStore((s) => s.settings?.hiddenApps ?? NO_APPS);
  const groups = useSettingsStore((s) => s.settings?.groups ?? NO_GROUPS);
  const pinApp = useSettingsStore((s) => s.pinApp);
  const unpinApp = useSettingsStore((s) => s.unpinApp);
  const hideApp = useSettingsStore((s) => s.hideApp);
  const createGroup = useSettingsStore((s) => s.createGroup);
  const addToGroup = useSettingsStore((s) => s.addToGroup);
  const removeFromGroup = useSettingsStore((s) => s.removeFromGroup);
  const updateGroup = useSettingsStore((s) => s.updateGroup);
  const deleteGroup = useSettingsStore((s) => s.deleteGroup);
  const renameApp = useSettingsStore((s) => s.renameApp);
  const appAliases = useSettingsStore((s) => s.settings?.appAliases ?? NO_ALIASES);
  const aliases = new Map(appAliases.map((a) => [a.appId, a.alias]));
  const scenes = useSettingsStore((s) => s.settings?.scenes ?? NO_SCENES);
  const createScene = useSettingsStore((s) => s.createScene);
  const updateScene = useSettingsStore((s) => s.updateScene);
  const deleteScene = useSettingsStore((s) => s.deleteScene);
  const syncGroupMembers = useSettingsStore((s) => s.syncGroupMembers);
  const setAppVolume = useAudioStore((s) => s.setAppVolume);
  const setAppMute = useAudioStore((s) => s.setAppMute);

  // 列表由三部分组成：置顶的应用、分组、其余应用（后端顺序：活跃在前，再按名称）。隐藏的不显示。
  const hidden = new Set(hiddenApps.map((a) => a.appId));
  const all = [...(snapshot?.apps ?? []), ...(debugTools ? debugApps : [])].filter(
    (app) => !hidden.has(app.appId),
  );
  const grouped = new Set(groups.flatMap((g) => g.apps.map((a) => a.appId)));
  const pinned = pinnedApps.flatMap(
    (p) => all.find((a) => a.appId === p.appId && !grouped.has(a.appId)) ?? [],
  );
  const pinnedIds = new Set(pinned.map((a) => a.appId));
  const others = all.filter((a) => !pinnedIds.has(a.appId) && !grouped.has(a.appId));
  const groupApps = (group: AppGroup) =>
    group.apps.flatMap((m) => all.find((a) => a.appId === m.appId) ?? []);

  type Entry =
    | { kind: "app"; app: AppAudio; placement: Placement }
    | { kind: "group"; group: AppGroup };
  const entries: Entry[] = [
    ...pinned.map((app): Entry => ({ kind: "app", app, placement: { kind: "pinned" } })),
    ...groups.map((group): Entry => ({ kind: "group", group })),
    ...others.map((app): Entry => ({ kind: "app", app, placement: { kind: "list" } })),
  ];
  // 倒序：整个列表反过来，置顶的和正在播放的都靠近底部（任务栏）。
  if (appsReversed) entries.reverse();

  const findApp = (appId: string) => all.find((a) => a.appId === appId);
  const savedOf = (appId: string): SavedApp => ({ appId, name: findApp(appId)?.name ?? appId });
  const volumeOf = (appId: string) => findApp(appId)?.volume.volume ?? 1;

  /** 列表中可记住的应用（不含以 PID 标识的和调试占位应用）的当前音量，保存为场景。 */
  const captureScene = (): SceneApp[] =>
    all
      .filter((a) => canRemember(a.appId))
      .map((a) => ({
        appId: a.appId,
        name: a.name,
        volume: a.volume.volume,
        muted: a.volume.muted,
      }));
  /** 应用场景：只调节正在运行的应用，分组成员同时更新它在组音量 100% 时的音量。 */
  const applyScene = (scene: Scene) => {
    const changed: [string, number][] = [];
    for (const saved of scene.apps) {
      const app = findApp(saved.appId);
      if (!app) continue;
      if (Math.round(app.volume.volume * 100) !== Math.round(saved.volume * 100)) {
        setAppVolume(saved.appId, saved.volume);
      }
      if (app.volume.muted !== saved.muted) setAppMute(saved.appId, saved.muted);
      changed.push([saved.appId, saved.volume]);
    }
    void syncGroupMembers(changed);
  };

  /** 应用放到某个位置：置顶区、分组或普通区域。 */
  const moveTo = (appId: string, target: DropTarget) => {
    const inGroup = groups.some((g) => g.apps.some((m) => m.appId === appId));
    if (target.kind === "group") {
      void addToGroup(target.groupId, savedOf(appId), volumeOf(appId));
      return;
    }
    if (inGroup) void removeFromGroup(appId);
    if (target.kind === "pinned") void pinApp(savedOf(appId));
    else void unpinApp(appId);
  };
  const { drag, start: startDrag } = useAppDrag(moveTo);

  const [renaming, setRenaming] = useState<Renaming | null>(null);
  const finishRename = (name: string | null) => {
    const target = renaming;
    setRenaming(null);
    if (!target || name === null) return;
    if (target.kind === "group") {
      if (name) void updateGroup(target.id, { name });
    } else if (target.kind === "scene") {
      if (name) void updateScene(target.id, { name });
    } else {
      void renameApp(savedOf(target.id), name || null);
    }
  };

  // 右键菜单中的动作。回调在菜单关闭后才执行，通过 ref 读取最新状态。
  const latest = useRef({ moveTo, groups, savedOf, volumeOf, captureScene });
  latest.current = { moveTo, groups, savedOf, volumeOf, captureScene };
  useEffect(
    () =>
      onMenuAction((action) => {
        const { moveTo, groups, savedOf, volumeOf, captureScene } = latest.current;
        const [kind, ...rest] = action.split(":");
        const arg = rest.join(":");
        switch (kind) {
          case "pin":
            moveTo(arg, { kind: "pinned" });
            break;
          case "unpin":
          case "ungroup":
            moveTo(arg, { kind: "list" });
            break;
          case "hide":
            void hideApp(savedOf(arg));
            break;
          case "group": {
            // group:<分组标识>:<应用标识>，分组标识不含冒号。
            const [groupId, ...appId] = rest;
            moveTo(appId.join(":"), { kind: "group", groupId });
            break;
          }
          case "new-group":
            void createGroup().then((groupId) => {
              if (groupId) void addToGroup(groupId, savedOf(arg), volumeOf(arg));
            });
            break;
          case "toggle-group": {
            const group = groups.find((g) => g.id === arg);
            if (group) void updateGroup(arg, { expanded: !group.expanded });
            break;
          }
          case "rename-group":
            setRenaming({ kind: "group", id: arg });
            break;
          case "rename-app":
            setRenaming({ kind: "app", id: arg });
            break;
          case "reset-name":
            void renameApp(savedOf(arg), null);
            break;
          case "scene-overwrite":
            void updateScene(arg, { apps: captureScene() });
            break;
          case "scene-rename":
            setRenaming({ kind: "scene", id: arg });
            break;
          case "scene-delete":
            void deleteScene(arg);
            break;
          case "delete-group":
            void deleteGroup(arg);
            break;
        }
      }),
    [
      hideApp,
      createGroup,
      addToGroup,
      updateGroup,
      deleteGroup,
      renameApp,
      updateScene,
      deleteScene,
    ],
  );

  // 点击应用行以外的位置时收起详情浮窗；切换到设置页时也收起。
  useEffect(() => {
    document.addEventListener("pointerdown", closeAppDetailsOnPointerDown, true);
    return () => {
      document.removeEventListener("pointerdown", closeAppDetailsOnPointerDown, true);
      closeAppDetails();
    };
  }, []);

  const rootRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  useFitWindowHeight(rootRef, scrollRef, contentRef);

  // 倒序时列表可滚动，默认停在底部，先看到正在播放的应用。
  const loaded = snapshot !== null;
  useLayoutEffect(() => {
    const scroll = scrollRef.current;
    if (appsReversed && loaded && scroll) scroll.scrollTop = scroll.scrollHeight;
  }, [appsReversed, loaded]);

  return (
    <div ref={rootRef} className="relative flex h-full flex-col">
      <header className="flex items-center justify-between px-4 pt-3 pb-2">
        <h1 className="min-w-0 truncate text-sm font-semibold">更优雅的音量控制器</h1>
        <div className="flex items-center gap-0.5">
          {debugTools && (
            <>
              {debugApps.length > 0 && (
                <IconButton label="清除全部调试应用" onClick={clearDebugApps}>
                  <Trash2 size={16} />
                </IconButton>
              )}
              <IconButton label="添加调试应用" onClick={addDebugApp}>
                <Plus size={16} />
              </IconButton>
            </>
          )}
          <IconButton
            label="把当前音量保存为场景"
            disabled={!all.some((a) => canRemember(a.appId))}
            onClick={() =>
              void createScene(captureScene()).then((id) => {
                if (id) setRenaming({ kind: "scene", id });
              })
            }
          >
            <BookmarkPlus size={16} />
          </IconButton>
          <IconButton label="新建分组" onClick={() => void createGroup()}>
            <FolderPlus size={16} />
          </IconButton>
          <PinButton />
          <IconButton label="设置" onClick={onOpenSettings}>
            <SettingsIcon size={16} />
          </IconButton>
        </div>
      </header>

      {autostartPrompt && <AutostartPrompt />}
      {scenes.length > 0 && device && (
        <ScenesBar
          scenes={scenes}
          apps={all}
          aliases={aliases}
          renaming={renaming}
          onRenameDone={finishRename}
          onApply={applyScene}
        />
      )}

      {/* 系统音量固定在顶部（或底部），只有应用列表滚动。 */}
      {device && !masterAtBottom && (
        <>
          {masterSection}
          <h2 className="px-4 pt-4 pb-1 text-xs font-medium text-muted-foreground">应用</h2>
        </>
      )}
      {device && masterAtBottom && (
        <h2 className="px-4 pt-1 pb-1 text-xs font-medium text-muted-foreground">应用</h2>
      )}

      {/* layoutScroll：列表滚动后，排序动画仍能算对位置。 */}
      <motion.div
        ref={scrollRef}
        layoutScroll
        // 列表滚动后应用行移了位置，收起详情浮窗。
        onScroll={closeAppDetails}
        className="scroll-area min-h-0 flex-1 overflow-y-auto"
      >
        <div ref={contentRef} className={device && masterAtBottom ? "pb-2" : "pb-3"}>
          {snapshot === null && loadError ? (
            <EmptyState
              icon={<VolumeOff size={24} />}
              title="无法读取音频设备"
              hint={loadError}
              action={
                <button
                  type="button"
                  onClick={() => void retry()}
                  className="mt-1 rounded-md border border-border bg-card px-3 py-1 text-foreground hover:bg-accent"
                >
                  重试
                </button>
              }
            />
          ) : snapshot === null ? (
            <MainPageSkeleton masterAtBottom={masterAtBottom} />
          ) : !device ? (
            <EmptyState
              icon={<VolumeOff size={24} />}
              title="未检测到输出设备"
              hint="连接扬声器或耳机后会自动显示"
            />
          ) : entries.length === 0 ? (
            <EmptyState
              icon={<AudioLines size={24} animateOnView loop />}
              title="暂无正在使用声音的应用"
            />
          ) : (
            // 可滚动时滚轮用于滚动列表，不调节应用音量；系统音量不受影响。
            // 拖动应用时，整个列表是“普通区域”（取消置顶、移出分组）。
            <ul data-drop="list" className="relative flex flex-col gap-1 pr-1 pl-3">
              {/* 拖动时在列表开头（倒序时在末尾）显示置顶区。 */}
              {drag && !appsReversed && <PinnedDropZone drag={drag} />}
              {/* 首次显示不播放进入动画。 */}
              <AnimatePresence initial={false} mode="popLayout">
                {entries.map((entry) =>
                  entry.kind === "group" ? (
                    <GroupItem
                      key={entry.group.id}
                      group={entry.group}
                      apps={groupApps(entry.group)}
                      groups={groups}
                      aliases={aliases}
                      renaming={renaming}
                      onRenameDone={finishRename}
                      scrollAreaRef={scrollRef}
                      drag={drag}
                      onDragStart={(app, e) =>
                        startDrag(app.appId, aliases.get(app.appId) ?? app.name, "group", e)
                      }
                    />
                  ) : (
                    <AppItem
                      key={entry.app.appId}
                      app={entry.app}
                      alias={aliases.get(entry.app.appId)}
                      renaming={renaming?.kind === "app" && renaming.id === entry.app.appId}
                      onRenameDone={finishRename}
                      placement={entry.placement}
                      groups={groups}
                      scrollAreaRef={scrollRef}
                      dragging={drag?.appId === entry.app.appId}
                      detailsDisabled={!!drag || renaming !== null}
                      onDragStart={(e) =>
                        startDrag(
                          entry.app.appId,
                          aliases.get(entry.app.appId) ?? entry.app.name,
                          entry.placement.kind,
                          e,
                        )
                      }
                    />
                  ),
                )}
              </AnimatePresence>
              {drag && appsReversed && <PinnedDropZone drag={drag} />}
            </ul>
          )}
        </div>
      </motion.div>

      {device && masterAtBottom && <div className="pb-3">{masterSection}</div>}

      {drag && <DragBadge drag={drag} />}
      <ErrorToast />
    </div>
  );
}
