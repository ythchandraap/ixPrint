import { ref, type Ref } from "vue";
import type { Template, TemplateElement, TemplateGroup } from "@/shared/api";
import type { DesignNode } from "./useDesignLayers";
import { descendantElementIds, descendantGroupIds } from "./groupModel";

interface ClipboardTree { roots: DesignNode[]; elements: TemplateElement[]; groups: TemplateGroup[] }
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const createId = (used: Set<string>) => {
  let id = "";
  do id = crypto.randomUUID?.() ?? Math.random().toString(36).slice(2, 12);
  while (used.has(id));
  used.add(id);
  return id;
};

export function useDesignClipboard(
  template: Template,
  selected: Ref<DesignNode[]>,
  focusGroupId: Ref<string | null>,
  moveSelectionBy: (dx: number, dy: number) => void,
  markSelection: () => void,
) {
  const copied = ref<ClipboardTree | null>(null);
  let pasteCount = 0;

  function copySelected() {
    if (!selected.value.length) return;
    const groupIds = new Set<string>();
    const elementIds = new Set<string>();
    for (const node of selected.value) {
      if (node.kind === "group") {
        groupIds.add(node.id);
        descendantGroupIds(template, node.id).forEach((id) => groupIds.add(id));
      } else elementIds.add(node.id);
    }
    for (const groupId of [...groupIds]) descendantElementIds(template, groupId).forEach((id) => elementIds.add(id));
    copied.value = {
      roots: clone(selected.value),
      elements: clone(template.elements.filter((element) => elementIds.has(element.id))),
      groups: clone((template.groups ?? []).filter((group) => groupIds.has(group.id))),
    };
    pasteCount = 0;
  }

  function pasteCopied() {
    const source = copied.value;
    if (!source) return;
    const used = new Set([...template.elements.map((element) => element.id), ...(template.groups ?? []).map((group) => group.id)]);
    const idMap = new Map<string, string>();
    for (const element of source.elements) idMap.set(element.id, createId(used));
    for (const group of source.groups) idMap.set(group.id, createId(used));
    const clonedGroups = source.groups.map((group) => ({
      ...clone(group), id: idMap.get(group.id)!,
      parentGroupId: source.roots.some((root) => root.kind === "group" && root.id === group.id)
        ? focusGroupId.value : idMap.get(group.parentGroupId ?? "") ?? null,
      elementIds: group.elementIds.map((id) => idMap.get(id)!).filter(Boolean),
    }));
    const clonedElements = source.elements.map((element) => ({ ...clone(element), id: idMap.get(element.id)! }));
    for (const root of source.roots) if (root.kind === "element" && focusGroupId.value) {
      const parent = template.groups?.find((group) => group.id === focusGroupId.value);
      if (parent) parent.elementIds.push(idMap.get(root.id)!);
    }
    template.groups ??= [];
    template.groups.push(...clonedGroups);
    template.elements.push(...clonedElements);
    selected.value = source.roots.map((root) => ({ kind: root.kind, id: idMap.get(root.id)! }));
    pasteCount += 1;
    moveSelectionBy(2 * pasteCount, 2 * pasteCount);
    markSelection();
  }

  function duplicateSelected() { copySelected(); pasteCopied(); }
  return { copied, copySelected, pasteCopied, duplicateSelected };
}
