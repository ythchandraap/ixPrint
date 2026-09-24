import { onBeforeUnmount, type ComputedRef, type Ref } from "vue";
import type { TemplateElement } from "@/shared/api";

type Drag =
  | { kind: "move" | "resize"; index: number; startX: number; startY: number; origin: TemplateElement }
  | { kind: "end"; index: number; end: 1 | 2; startX: number; startY: number; origin: TemplateElement };
const snap = (value: number) => Math.round(value / 0.5) * 0.5;
const clone = (element: TemplateElement) => JSON.parse(JSON.stringify(element)) as TemplateElement;

export function useDesignPointer(
  elements: TemplateElement[],
  selected: Ref<number | null>,
  scale: ComputedRef<number>,
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
    if (element.type === "line") {
      element.x1 = snap(element.x1 + dx);
      element.x2 = snap(element.x2 + dx);
      element.y1 = snap(element.y1 + dy);
      element.y2 = snap(element.y2 + dy);
    } else {
      element.x = snap(element.x + dx);
      element.y = snap(element.y + dy);
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
      if (drag.end === 1) [element.x1, element.y1] = [snap(origin.x1 + dx), snap(origin.y1 + dy)];
      else [element.x2, element.y2] = [snap(origin.x2 + dx), snap(origin.y2 + dy)];
    } else if (drag.kind === "resize") {
      if (element.type === "qr" && origin.type === "qr")
        element.size = Math.max(2, snap(origin.size + Math.max(dx, dy)));
      else if (element.type !== "line" && element.type !== "qr" && origin.type !== "line" && origin.type !== "qr") {
        element.w = Math.max(1, snap(origin.w + dx));
        element.h = Math.max(1, snap(origin.h + dy));
      }
    }
  }

  function endDrag() {
    drag = null;
    window.removeEventListener("pointermove", onDrag);
  }

  function startDrag(event: PointerEvent, index: number, kind: "move" | "resize" | "end1" | "end2") {
    event.preventDefault();
    event.stopPropagation();
    selected.value = index;
    const origin = clone(elements[index]);
    const base = { index, startX: event.clientX, startY: event.clientY, origin };
    drag =
      kind === "end1" || kind === "end2" ? { kind: "end", end: kind === "end1" ? 1 : 2, ...base } : { kind, ...base };
    window.addEventListener("pointermove", onDrag);
    window.addEventListener("pointerup", endDrag, { once: true });
  }

  onBeforeUnmount(endDrag);
  return { bbox, moveBy, startDrag, endDrag };
}
