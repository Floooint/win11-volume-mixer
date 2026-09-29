import { useCallback, useEffect, useRef, useState } from "react";

/** 拖动时可以放下的位置。 */
export type DropTarget =
  | { kind: "pinned" }
  | { kind: "group"; groupId: string }
  /** 普通区域：取消置顶、移出分组。 */
  | { kind: "list" };

export type DragState = {
  appId: string;
  name: string;
  /** 应用原来所在的位置类型，用于提示。 */
  from: DropTarget["kind"];
  /** 光标相对视口的位置，浮动卡片跟随它移动。 */
  x: number;
  y: number;
  /** 光标所在的放下位置；不在任何位置上时为 `null`。 */
  target: DropTarget | null;
};

/** 移动超过这个距离才开始拖动，避免误把点击当成拖动。 */
const DRAG_THRESHOLD = 4;

/** 从光标下的元素向上找 `data-drop` 标记的放下位置。 */
function dropTargetAt(x: number, y: number): DropTarget | null {
  const element = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop]");
  const drop = element?.dataset.drop;
  if (!drop) return null;
  if (drop === "pinned" || drop === "list") return { kind: drop };
  if (drop.startsWith("group:")) return { kind: "group", groupId: drop.slice("group:".length) };
  return null;
}

/** 拖动结束后吞掉紧接着的一次点击：松手处的点击不应触发应用行的点击操作（如显示详情）。 */
function swallowNextClick() {
  const stop = (e: MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
  };
  window.addEventListener("click", stop, { capture: true, once: true });
  // 点击事件在 pointerup 之后同步派发；没有点击（如松手位置不同）时下一轮移除监听。
  setTimeout(() => window.removeEventListener("click", stop, { capture: true }));
}

/**
 * 应用拖放：从拖动手柄上按下并移动后开始拖动，松手时在光标所在的放下位置调用 `onDrop`。
 * 放下位置用 `data-drop="pinned" | "list" | "group:<id>"` 标记。按 Esc、窗口失焦或
 * 丢失指针（pointercancel、移动时已没有按键按下）时取消。
 */
export function useAppDrag(onDrop: (appId: string, target: DropTarget) => void) {
  const [drag, setDrag] = useState<DragState | null>(null);
  // 拖动状态同时放在 ref 中，事件处理直接读取；state 只用于渲染。
  const dragRef = useRef<DragState | null>(null);
  const onDropRef = useRef(onDrop);
  onDropRef.current = onDrop;
  const pending = useRef<{
    appId: string;
    name: string;
    from: DropTarget["kind"];
    x: number;
    y: number;
  } | null>(null);

  useEffect(() => {
    const update = (next: DragState | null) => {
      dragRef.current = next;
      setDrag(next);
    };
    const cancel = () => {
      pending.current = null;
      if (dragRef.current) update(null);
    };
    const move = (e: PointerEvent) => {
      const start = pending.current;
      if (!start) return;
      // 松手事件丢失（如在窗口外松开）：鼠标移回时已没有按键按下，取消拖动。
      if (e.buttons === 0) {
        cancel();
        return;
      }
      if (
        !dragRef.current &&
        Math.hypot(e.clientX - start.x, e.clientY - start.y) < DRAG_THRESHOLD
      ) {
        return;
      }
      update({
        appId: start.appId,
        name: start.name,
        from: start.from,
        x: e.clientX,
        y: e.clientY,
        target: dropTargetAt(e.clientX, e.clientY),
      });
    };
    const end = (e: PointerEvent) => {
      const start = pending.current;
      const dragged = dragRef.current !== null;
      pending.current = null;
      if (!dragged) return;
      update(null);
      swallowNextClick();
      const target = dropTargetAt(e.clientX, e.clientY);
      if (start && target) onDropRef.current(start.appId, target);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") cancel();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("blur", cancel);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  /** 在拖动手柄的 `onPointerDown` 中调用。 */
  const start = useCallback(
    (appId: string, name: string, from: DropTarget["kind"], e: React.PointerEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      pending.current = { appId, name, from, x: e.clientX, y: e.clientY };
    },
    [],
  );

  return { drag, start };
}

/** 两个放下位置是否相同。 */
export function sameTarget(a: DropTarget | null, b: DropTarget): boolean {
  if (!a || a.kind !== b.kind) return false;
  return a.kind !== "group" || (b.kind === "group" && a.groupId === b.groupId);
}
