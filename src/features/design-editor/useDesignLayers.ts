import { computed, ref, type ComputedRef } from "vue";
import type { Template, TemplateElement, TemplateGroup } from "@/shared/api";
import { descendantElementIds, descendantGroupIds, groupPath, parentOfNode, resolveElementNode, validateGroups } from "./groupModel";

type ListLayout = Extract<NonNullable<Template["layout"]>, { mode: "list" }>;

const snap = (value: number) => Math.round(value / 0.5) * 0.5;
export type DesignNode = { kind: "element" | "group"; id: string };
const newId = (used: Set<string>) => {
  let id = "";
  do id = crypto.randomUUID?.() ?? Math.random().toString(36).slice(2, 12);
  while (used.has(id));
  used.add(id);
  return id;
};

export function useDesignLayers(
  template: Template,
  list: ComputedRef<ListLayout | null>,
  canvasHeight: ComputedRef<number>,
  ensureSample: (field: string) => void,
) {
  if (!template.groups) template.groups = [];
  const selected = ref<DesignNode[]>([]);
  const focusGroupId = ref<string | null>(null);
  const selectedIndex = computed(() => selected.value.length === 1 && selected.value[0].kind === "element"
    ? template.elements.findIndex((element) => element.id === selected.value[0].id)
    : -1);
  const selectedElementId = computed(() => selectedIndex.value < 0 ? null : selected.value[0].id);
  const current = computed(() => selectedIndex.value < 0 ? null : template.elements[selectedIndex.value]);
  const groupPathRows = computed(() => groupPath(template, focusGroupId.value));
  const layerRows = computed(() => template.elements.map((element, index) => ({
    element, index, label: layerLabel(element), groupPath: groupPath(template, parentOfNode(template, { kind: "element", id: element.id }) ?? null).map((item) => item.name).join(" / "),
  })).reverse());
  const structureRows = computed(() => {
    const elementIndex = new Map(template.elements.map((element, index) => [element.id, index]));
    const groupMinIndex = (groupId: string) => Math.min(...[...descendantElementIds(template, groupId)].map((id) => elementIndex.get(id) ?? Infinity));
    const rows: { node: DesignNode; depth: number; label: string; group?: TemplateGroup; hasChildren: boolean; sortIndex: number }[] = [];
    const visited = new Set<string>();
    const append = (parent: string | null, depth: number) => {
      const groups = template.groups!.filter((group) => group.parentGroupId === parent)
        .map((group) => ({ group, sortIndex: groupMinIndex(group.id) }));
      const elements = template.elements.filter((element) => parentOfNode(template, { kind: "element", id: element.id }) === parent)
        .map((element) => ({ element, sortIndex: elementIndex.get(element.id)! }));
      const nodes: { node: DesignNode; depth: number; label: string; group?: TemplateGroup; hasChildren: boolean; sortIndex: number }[] = [
        ...groups.map(({ group, sortIndex }) => ({ node: { kind: "group" as const, id: group.id }, depth, label: group.name, group, hasChildren: true, sortIndex })),
        ...elements.map(({ element, sortIndex }) => ({ node: { kind: "element" as const, id: element.id }, depth, label: layerLabel(element), hasChildren: false, sortIndex })),
      ].sort((a, b) => a.sortIndex - b.sortIndex);
      for (const node of nodes) {
        rows.push(node);
        if (node.group && !visited.has(node.group.id)) { visited.add(node.group.id); append(node.group.id, depth + 1); }
      }
    };
    append(null, 0);
    return rows;
  });
  const moveTargets = computed(() => {
    const selectedGroups = selected.value.filter((node) => node.kind === "group").map((node) => node.id);
    return [
      { id: null as string | null, label: "Canvas" },
      ...template.groups!.filter((group) => !selectedGroups.some((id) => id === group.id || descendantGroupIds(template, id).has(group.id)))
        .map((group) => ({ id: group.id, label: groupPath(template, group.id).map((item) => item.name).join(" / ") })),
    ];
  });

  let markSelection: () => void = () => {};
  function clearSelection() { selected.value = []; markSelection(); }
  function selectNode(node: DesignNode, additive = false) {
    const valid = parentOfNode(template, node) === focusGroupId.value;
    if (!valid) return;
    if (additive) {
      selected.value = selected.value.some((item) => item.kind === node.kind && item.id === node.id)
        ? selected.value.filter((item) => item.kind !== node.kind || item.id !== node.id)
        : [...selected.value, node];
    } else selected.value = [node];
    markSelection();
  }
  function selectFromStructure(node: DesignNode) {
    focusGroupId.value = parentOfNode(template, node) ?? null;
    selected.value = [node];
    markSelection();
  }
  function resolveElement(elementId: string) { return resolveElementNode(template, elementId, focusGroupId.value); }
  function enterGroup(groupId: string) {
    const group = template.groups!.find((item) => item.id === groupId);
    if (!group || group.parentGroupId !== focusGroupId.value) return;
    focusGroupId.value = groupId;
    selected.value = [];
    markSelection();
  }
  function setFocus(groupId: string | null) {
    if (groupId !== null && !template.groups!.some((group) => group.id === groupId)) return;
    focusGroupId.value = groupId;
    selected.value = [];
    markSelection();
  }
  function exitFocus() {
    if (focusGroupId.value === null) { clearSelection(); return; }
    const group = template.groups!.find((item) => item.id === focusGroupId.value);
    focusGroupId.value = group?.parentGroupId ?? null;
    selected.value = [];
    markSelection();
  }
  function setMarkSelection(callback: () => void) { markSelection = callback; }

  function bandOf(element: TemplateElement): "header" | "row" | "footer" {
    const layout = list.value;
    if (!layout) return "row";
    const top = element.type === "line" ? Math.min(element.y1, element.y2) : element.y;
    return top < layout.header_mm - 0.001
      ? "header"
      : top < layout.header_mm + layout.row_mm - 0.001
        ? "row"
        : "footer";
  }

  function bandRange(band: "header" | "row" | "footer"): [number, number] {
    const layout = list.value;
    if (!layout) return [0, canvasHeight.value];
    if (band === "header") return [0, layout.header_mm];
    if (band === "row") return [layout.header_mm, layout.header_mm + layout.row_mm];
    return [layout.header_mm + layout.row_mm, layout.header_mm + layout.row_mm + layout.footer_mm];
  }
  function focusBand(): "header" | "row" | "footer" {
    if (!list.value || !focusGroupId.value) return "row";
    const ids = descendantElementIds(template, focusGroupId.value);
    const first = template.elements.find((element) => ids.has(element.id));
    return first ? bandOf(first) : "row";
  }
  function add(type: Exclude<TemplateElement["type"], "image">): string | null {
    const width = template.width_mm;
    const height = canvasHeight.value;
    const margin = Math.min(2, width / 10);
    const band = focusBand();
    const [bandStart, bandEnd] = bandRange(band);
    const y0 = list.value ? bandStart : margin;
    const availableHeight = bandEnd - y0;
    if (list.value && availableHeight < 1) return `The ${band} band is too small for a new object`;
    const used = new Set([...template.elements.map((item) => item.id), ...template.groups!.map((item) => item.id)]);
    const element: TemplateElement =
      type === "text"
        ? {
            type,
            id: newId(used),
            x: margin,
            y: y0,
            w: width - 2 * margin,
            h: list.value ? Math.min(8, availableHeight) : Math.min(8, height - 2 * margin),
            text: "Text {{kolom}}",
            size: 10,
            font: "instrument_sans",
            weight: "regular",
            bold: false,
            align: "left",
            valign: "top",
            wrap: true,
            fit: true,
          }
        : type === "line"
          ? {
              type,
              id: newId(used),
              x1: margin,
              y1: snap(list.value ? (bandStart + bandEnd) / 2 : height / 2),
              x2: width - margin,
              y2: snap(list.value ? (bandStart + bandEnd) / 2 : height / 2),
              width: 0.75,
            }
          : type === "rect"
            ? {
                type,
                id: newId(used),
                x: margin,
                y: y0,
                w: width - 2 * margin,
                h: list.value ? Math.max(1, availableHeight) : height - 2 * margin,
                stroke: 0.75,
                fill: false,
              }
            : type === "barcode"
              ? {
                  type,
                  id: newId(used),
                  x: margin,
                  y: y0,
                  w: Math.min(50, width - 2 * margin),
                  h: list.value ? Math.max(1, Math.min(12, availableHeight)) : Math.min(12, height - 2 * margin),
                  value: "{{kode}}",
                  show_text: true,
                }
              : {
                  type,
                  id: newId(used),
                  x: margin,
                  y: y0,
                  size: Math.min(20, width - 2 * margin, list.value ? availableHeight : height - 2 * margin),
                  value: "{{kode}}",
                };
    template.elements.push(element);
    if (focusGroupId.value) template.groups!.find((group) => group.id === focusGroupId.value)?.elementIds.push(element.id);
    if (element.type === "text") ensureSample("kolom");
    else if (element.type === "barcode" || element.type === "qr") ensureSample("kode");
    selected.value = [{ kind: "element", id: element.id }];
    markSelection();
    return null;
  }

  function createGroup(): string | null {
    if (selected.value.length < 2 || selected.value.some((node) => parentOfNode(template, node) !== focusGroupId.value)) return "Select at least two sibling nodes";
    const err = validateNodesBands(selected.value);
    if (err) return err;
    const usedIds = new Set([...template.elements.map((item) => item.id), ...template.groups!.map((item) => item.id)]);
    const siblings = template.groups!.filter((group) => group.parentGroupId === focusGroupId.value);
    const names = new Set(siblings.map((group) => group.name));
    let number = 1;
    while (names.has(`Group ${number}`)) number++;
    const id = newId(usedIds);
    const group: TemplateGroup = { id, name: `Group ${number}`, parentGroupId: focusGroupId.value, elementIds: [] };
    for (const node of selected.value) {
      if (node.kind === "element") {
        if (focusGroupId.value) {
          const parent = template.groups!.find((item) => item.id === focusGroupId.value)!;
          parent.elementIds = parent.elementIds.filter((item) => item !== node.id);
        }
        group.elementIds.push(node.id);
      } else template.groups!.find((item) => item.id === node.id)!.parentGroupId = id;
    }
    template.groups!.push(group);
    selected.value = [{ kind: "group", id }];
    markSelection();
    return null;
  }

  function ungroupSelection(): string | null {
    const node = selected.value.length === 1 ? selected.value[0] : null;
    if (!node || node.kind !== "group") return "Select one group to ungroup";
    const group = template.groups!.find((item) => item.id === node.id);
    if (!group) return "Group no longer exists";
    const members: DesignNode[] = [
      ...group.elementIds.map((id) => ({ kind: "element" as const, id })),
      ...template.groups!.filter((item) => item.parentGroupId === group.id).map((item) => ({ kind: "group" as const, id: item.id })),
    ];
    if (group.parentGroupId) {
      const parent = template.groups!.find((item) => item.id === group.parentGroupId)!;
      parent.elementIds.push(...group.elementIds);
    }
    for (const child of template.groups!) if (child.parentGroupId === group.id) child.parentGroupId = group.parentGroupId;
    template.groups!.splice(template.groups!.indexOf(group), 1);
    focusGroupId.value = group.parentGroupId;
    selected.value = members;
    markSelection();
    return null;
  }

  function moveSelectionTo(targetId: string | null): string | null {
    if (!selected.value.length) return "Select an object or group first";
    if (selected.value.some((node) => parentOfNode(template, node) !== focusGroupId.value)) return "Selection is no longer at the active level";
    if (targetId !== null && !template.groups!.some((group) => group.id === targetId)) return "Destination group no longer exists";
    if (selected.value.some((node) => node.kind === "group" && (node.id === targetId || descendantGroupIds(template, node.id).has(targetId ?? "")))) {
      return "Moving a group into itself or one of its descendants would create a cycle";
    }
    const err = validateNodesBands(selected.value, targetId);
    if (err) return err;
    const movedElements = selected.value.filter((node) => node.kind === "element").map((node) => node.id);
    const movedGroups = selected.value.filter((node) => node.kind === "group").map((node) => node.id);
    for (const parent of template.groups!) parent.elementIds = parent.elementIds.filter((id) => !movedElements.includes(id));
    for (const id of movedGroups) template.groups!.find((group) => group.id === id)!.parentGroupId = targetId;
    if (targetId) {
      const target = template.groups!.find((group) => group.id === targetId)!;
      target.elementIds.push(...movedElements);
    }
    cleanupEmptyGroups();
    focusGroupId.value = targetId;
    markSelection();
    return null;
  }

  function validateNodesBands(nodes: DesignNode[], targetId?: string | null): string | null {
    if (!list.value) return null;
    if (targetId === null) return null;
    const ids = new Set(nodes.flatMap((node) => node.kind === "element" ? [node.id] : [...descendantElementIds(template, node.id)]));
    const bands = new Set(template.elements.filter((element) => ids.has(element.id)).map((element) => bandOf(element)));
    const target = targetId === undefined ? null : targetId;
    if (target) {
      for (const id of descendantElementIds(template, target)) {
        const element = template.elements.find((item) => item.id === id);
        if (element) bands.add(bandOf(element));
      }
    }
    if (bands.size > 1) return `Operation rejected: group members conflict across list bands (${[...bands].join(", ")})`;
    return null;
  }

  function cleanupEmptyGroups() {
    const parents = new Map(template.groups!.map((group) => [group.id, group.parentGroupId]));
    let removed = true;
    while (removed) {
      removed = false;
      for (const group of [...template.groups!]) if (!descendantElementIds(template, group.id).size) {
        template.groups!.splice(template.groups!.indexOf(group), 1);
        removed = true;
      }
    }
    if (focusGroupId.value && !template.groups!.some((group) => group.id === focusGroupId.value)) {
      let id: string | null = focusGroupId.value;
      while (id && !template.groups!.some((group) => group.id === id)) id = parents.get(id) ?? null;
      focusGroupId.value = id;
      selected.value = [];
    }
  }

  function removeSelected() {
    if (!selected.value.length) return 0;
    const removeIds = new Set(selected.value.flatMap((node) => node.kind === "element" ? [node.id] : [...descendantElementIds(template, node.id)]));
    const removeGroups = new Set(selected.value.flatMap((node) => node.kind === "group" ? [node.id, ...descendantGroupIds(template, node.id)] : []));
    template.elements.splice(0, template.elements.length, ...template.elements.filter((element) => !removeIds.has(element.id)));
    for (const group of template.groups!) {
      group.elementIds = group.elementIds.filter((id) => !removeIds.has(id));
    }
    template.groups = template.groups!.filter((group) => !removeGroups.has(group.id));
    selected.value = [];
    cleanupEmptyGroups();
    markSelection();
    return removeIds.size;
  }

  function reorderAt(index: number, direction: -1 | 1) {
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= template.elements.length) return;
    [template.elements[index], template.elements[nextIndex]] = [template.elements[nextIndex], template.elements[index]];
  }

  function setMode(mode: "label" | "list"): string | null {
    if (mode === "label") template.layout = { mode: "label" };
    else if (!list.value) {
      const header = snap(Math.min(20, template.height_mm / 4));
      const candidate: Template = { ...template, layout: { mode: "list", header_mm: header, row_mm: 10, footer_mm: 15, footer: "last_page", roll: false } };
      const error = validateGroups(candidate);
      if (error) return error;
      template.layout = candidate.layout;
    }
    return null;
  }

  function renameGroup(groupId: string, name: string): string | null {
    const normalized = name.trim();
    if (!normalized || normalized.length > 60) return "Group name must contain 1–60 characters";
    const group = template.groups!.find((item) => item.id === groupId);
    if (!group) return "Group no longer exists";
    group.name = normalized;
    return null;
  }

  function selectAndReorder(index: number, direction: -1 | 1) {
    const element = template.elements[index];
    if (!element || parentOfNode(template, { kind: "element", id: element.id }) !== focusGroupId.value) return;
    selectFromStructure({ kind: "element", id: element.id });
    reorderAt(index, direction);
  }

  function restoreEditorState(state: { selected?: DesignNode[]; focusGroupId?: string | null }) {
    let id = state.focusGroupId ?? null;
    while (id && !template.groups!.some((group) => group.id === id)) id = template.groups!.find((group) => group.id === id)?.parentGroupId ?? null;
    focusGroupId.value = id;
    selected.value = (state.selected ?? []).filter((node) => parentOfNode(template, node) === focusGroupId.value);
  }

  return {
    selected, selectedIndex, selectedElementId, current, focusGroupId, groupPathRows, layerRows, structureRows,
    moveTargets, setMode, bandOf, add, removeSelected, reorderAt, selectAndReorder, selectNode, selectFromStructure,
    resolveElement, enterGroup, setFocus, exitFocus, clearSelection, createGroup, ungroupSelection, moveSelectionTo,
    renameGroup, cleanupEmptyGroups, setMarkSelection, restoreEditorState,
  };
}

function layerLabel(element: TemplateElement) {
  const field = (value: string) => value.match(/\{\{\s*([^{}]+?)\s*\}\}/)?.[1];
  switch (element.type) {
    case "text":
      return `Text${field(element.text) ? ` · ${field(element.text)}` : ""}`;
    case "line":
      return "Line";
    case "rect":
      return "Rectangle";
    case "barcode":
      return `Barcode${field(element.value) ? ` · ${field(element.value)}` : ""}`;
    case "qr":
      return `QR${field(element.value) ? ` · ${field(element.value)}` : ""}`;
    case "image":
      return `Image${field(element.src) ? ` · ${field(element.src)}` : ""}`;
  }
  return "Element";
}
