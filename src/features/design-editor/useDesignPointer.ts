import { onBeforeUnmount, type ComputedRef, type Ref } from "vue";
import type { Template, TemplateElement } from "@/shared/api";
import { elementBox } from "./groupModel";
import type { DesignNode } from "./useDesignLayers";

export type ResizeHandle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
export type PointerAction = "move" | "end1" | "end2" | ResizeHandle;
type Drag =
  | { kind: "move"; ids: string[]; startX: number; startY: number; origins: Map<string, TemplateElement> }
  | { kind: "resize"; handle: ResizeHandle; id: string; startX: number; startY: number; origin: TemplateElement }
  | { kind: "end"; id: string; end: 1 | 2; startX: number; startY: number; origin: TemplateElement };
const snap = (value: number) => Math.round(value / 0.5) * 0.5;
const clone = (element: TemplateElement) => JSON.parse(JSON.stringify(element)) as TemplateElement;

export function useDesignPointer(
  template: Template,
  selection: Ref<DesignNode[]>,
  selectedIndex: ComputedRef<number>,
  scale: ComputedRef<number>,
  snapEnabled: Ref<boolean>,
  nodeElementIds: (node: DesignNode) => Set<string>,
  resolveElement: (id: string) => DesignNode | null,
  selectNode: (node: DesignNode, additive?: boolean) => void,
  constrainMove: (ids: string[], dx: number, dy: number) => [number, number],
  history: { beginTransaction: () => void; endTransaction: () => void },
) {
  let drag: Drag | null = null;
  const bbox = elementBox;

  function translate(element: TemplateElement, dx: number, dy: number) {
    if (element.type === "line") {
      element.x1 += dx; element.x2 += dx; element.y1 += dy; element.y2 += dy;
    } else { element.x += dx; element.y += dy; }
  }

  function applyDelta(ids: string[], rawDx: number, rawDy: number) {
    const uniqueIds = [...new Set(ids)];
    const dx = snapEnabled.value ? snap(rawDx) : Math.round(rawDx * 100) / 100;
    const dy = snapEnabled.value ? snap(rawDy) : Math.round(rawDy * 100) / 100;
    const [limitedX, limitedY] = constrainMove(uniqueIds, dx, dy);
    for (const id of uniqueIds) {
      const element = template.elements.find((item) => item.id === id);
      if (element) translate(element, limitedX, limitedY);
    }
    return [limitedX, limitedY] as const;
  }

  function selectedElementIds() {
    return [...new Set(selection.value.flatMap((node) => [...nodeElementIds(node)]))];
  }

  function moveSelectionBy(dx: number, dy: number) {
    return applyDelta(selectedElementIds(), dx, dy);
  }

  function moveBy(element: TemplateElement, dx: number, dy: number) {
    return applyDelta([element.id], dx, dy);
  }

  function onDrag(event: PointerEvent) {
    if (!drag) return;
    const active = drag;
    const dx = (event.clientX - active.startX) / scale.value;
    const dy = (event.clientY - active.startY) / scale.value;
    if (active.kind === "move") {
      const [limitedX, limitedY] = constrainMove(active.ids, snapEnabled.value ? snap(dx) : dx, snapEnabled.value ? snap(dy) : dy);
      for (const id of active.ids) {
        const element = template.elements.find((item) => item.id === id);
        const origin = active.origins.get(id);
        if (element && origin) { Object.assign(element, clone(origin)); translate(element, limitedX, limitedY); }
      }
      return;
    }
    const element = template.elements.find((item) => item.id === active.id);
    if (!element) return;
    const origin = active.origin;
    const position = (value: number) => snapEnabled.value ? snap(value) : Math.round(value * 100) / 100;
    if (active.kind === "end" && element.type === "line" && origin.type === "line") {
      Object.assign(element, clone(origin));
      if (active.end === 1) [element.x1, element.y1] = [position(origin.x1 + dx), position(origin.y1 + dy)];
      else [element.x2, element.y2] = [position(origin.x2 + dx), position(origin.y2 + dy)];
    } else if (active.kind === "resize") {
      Object.assign(element, clone(origin));
      if (element.type === "qr" && origin.type === "qr")
        element.size = Math.max(2, position(origin.size + Math.max(dx, dy)));
      else if (element.type !== "line" && element.type !== "qr" && origin.type !== "line" && origin.type !== "qr") {
        const right = origin.x + origin.w;
        const bottom = origin.y + origin.h;
        const left = active.handle.includes("w") ? Math.min(position(origin.x + dx), right - 1) : origin.x;
        const top = active.handle.includes("n") ? Math.min(position(origin.y + dy), bottom - 1) : origin.y;
        const resizedRight = active.handle.includes("e") ? Math.max(position(right + dx), origin.x + 1) : right;
        const resizedBottom = active.handle.includes("s") ? Math.max(position(bottom + dy), origin.y + 1) : bottom;
        element.x = left; element.y = top; element.w = Math.round((resizedRight - left) * 100) / 100;
        element.h = Math.round((resizedBottom - top) * 100) / 100;
      }
    }
  }

  function endDrag() {
    if (!drag) return;
    drag = null;
    window.removeEventListener("pointermove", onDrag);
    window.removeEventListener("pointerup", endDrag);
    window.removeEventListener("pointercancel", endDrag);
    history.endTransaction();
  }

  function startDrag(event: PointerEvent, index: number, kind: PointerAction) {
    const element = template.elements[index];
    if (!element) return;
    const node = resolveElement(element.id);
    if (!node) return;
    event.preventDefault();
    event.stopPropagation();
    (event.currentTarget as HTMLElement).closest<HTMLElement>(".stage")?.focus();
    if (event.shiftKey) { selectNode(node, true); return; }
    const selected = selection.value.some((item) => item.kind === node.kind && item.id === node.id);
    if (!selected) selectNode(node);
    const selectedNodes = selected ? selection.value : [node];
    history.beginTransaction();
    if (kind === "move") {
      const ids = [...new Set(selectedNodes.flatMap((item) => [...nodeElementIds(item)]))];
      drag = { kind: "move", ids, startX: event.clientX, startY: event.clientY,
        origins: new Map(ids.flatMap((id) => { const item = template.elements.find((entry) => entry.id === id); return item ? [[id, clone(item)] as const] : []; })) };
    } else {
      selectNode({ kind: "element", id: element.id });
      const origin = clone(element);
      const base = { id: element.id, startX: event.clientX, startY: event.clientY, origin };
      drag = kind === "end1" || kind === "end2"
        ? { kind: "end", end: kind === "end1" ? 1 : 2, ...base }
        : { kind: "resize", handle: kind, ...base };
    }
    window.addEventListener("pointermove", onDrag);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);
  }

  onBeforeUnmount(endDrag);
  return { bbox, moveBy, moveSelectionBy, startDrag, endDrag, selectedIndex };
}
