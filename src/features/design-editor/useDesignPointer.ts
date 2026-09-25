import { onBeforeUnmount, type ComputedRef, type Ref } from "vue";
import type { TemplateElement } from "@/shared/api";

export type ResizeHandle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
export type PointerAction = "move" | "end1" | "end2" | ResizeHandle;

type Drag =
  | { kind: "move"; index: number; startX: number; startY: number; origin: TemplateElement }
  | { kind: "resize"; handle: ResizeHandle; index: number; startX: number; startY: number; origin: TemplateElement }
  | { kind: "end"; index: number; end: 1 | 2; startX: number; startY: number; origin: TemplateElement };
const snap = (value: number) => Math.round(value / 0.5) * 0.5;
const clone = (element: TemplateElement) => JSON.parse(JSON.stringify(element)) as TemplateElement;

export function useDesignPointer(
  elements: TemplateElement[],
  selected: Ref<number | null>,
  scale: ComputedRef<number>,
  snapEnabled: Ref<boolean>,
) {
  let drag: Drag | null = null;

  function bbox(element: TemplateElement) {
    switch (element.type) {
      case "line":
        return {
          x: Math.min(element.x1, element.x2),
          y: Math.min(element.y1, element.y2),
          w: Math.max(Math.abs(element.x2 - element.x1), 0.8),
          h: Math.max(Math.abs(element.y2 - element.y1), 0.8),
        };
      case "qr":
        return { x: element.x, y: element.y, w: element.size, h: element.size };
      default:
        return { x: element.x, y: element.y, w: element.w, h: element.h };
    }
  }

  function moveBy(element: TemplateElement, dx: number, dy: number) {
    const position = (value: number) => snapEnabled.value ? snap(value) : Math.round(value * 100) / 100;
    if (element.type === "line") {
      element.x1 = position(element.x1 + dx);
      element.x2 = position(element.x2 + dx);
      element.y1 = position(element.y1 + dy);
      element.y2 = position(element.y2 + dy);
    } else {
      element.x = position(element.x + dx);
      element.y = position(element.y + dy);
    }
  }

  function onDrag(event: PointerEvent) {
    if (!drag) return;
    const dx = (event.clientX - drag.startX) / scale.value;
    const dy = (event.clientY - drag.startY) / scale.value;
    const origin = drag.origin;
    const element = elements[drag.index];
    if (drag.kind === "move") {
      Object.assign(element, clone(origin));
      moveBy(element, dx, dy);
    } else if (drag.kind === "end" && element.type === "line" && origin.type === "line") {
      const position = (value: number) => snapEnabled.value ? snap(value) : Math.round(value * 100) / 100;
      if (drag.end === 1) [element.x1, element.y1] = [position(origin.x1 + dx), position(origin.y1 + dy)];
      else [element.x2, element.y2] = [position(origin.x2 + dx), position(origin.y2 + dy)];
    } else if (drag.kind === "resize") {
      const position = (value: number) => snapEnabled.value ? snap(value) : Math.round(value * 100) / 100;
      if (element.type === "qr" && origin.type === "qr")
        element.size = Math.max(2, position(origin.size + Math.max(dx, dy)));
      else if (element.type !== "line" && element.type !== "qr" && origin.type !== "line" && origin.type !== "qr") {
        const right = origin.x + origin.w;
        const bottom = origin.y + origin.h;
        const left = drag.handle.includes("w") ? Math.min(position(origin.x + dx), right - 1) : origin.x;
        const top = drag.handle.includes("n") ? Math.min(position(origin.y + dy), bottom - 1) : origin.y;
        const resizedRight = drag.handle.includes("e") ? Math.max(position(right + dx), origin.x + 1) : right;
        const resizedBottom = drag.handle.includes("s") ? Math.max(position(bottom + dy), origin.y + 1) : bottom;
        element.x = left;
        element.y = top;
        element.w = Math.round((resizedRight - left) * 100) / 100;
        element.h = Math.round((resizedBottom - top) * 100) / 100;
      }
    }
  }

  function endDrag() {
    drag = null;
    window.removeEventListener("pointermove", onDrag);
    window.removeEventListener("pointerup", endDrag);
    window.removeEventListener("pointercancel", endDrag);
  }

  function startDrag(event: PointerEvent, index: number, kind: PointerAction) {
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).closest<HTMLElement>(".stage")?.focus();
    selected.value = index;
    const origin = clone(elements[index]);
    const base = { index, startX: event.clientX, startY: event.clientY, origin };
    drag = kind === "end1" || kind === "end2"
      ? { kind: "end", end: kind === "end1" ? 1 : 2, ...base }
      : kind === "move" ? { kind: "move", ...base } : { kind: "resize", handle: kind, ...base };
    window.addEventListener("pointermove", onDrag);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);
  }

  onBeforeUnmount(endDrag);
  return { bbox, moveBy, startDrag, endDrag };
}
