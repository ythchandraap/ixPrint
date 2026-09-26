import type { Template, TemplateElement, TemplateGroup } from "@/shared/api";
import type { DesignNode } from "./useDesignLayers";

export function parentOfNode(template: Template, node: DesignNode): string | null {
  if (node.kind === "group") return template.groups?.find((group) => group.id === node.id)?.parentGroupId ?? null;
  return template.groups?.find((group) => group.elementIds.includes(node.id))?.id ?? null;
}

export function descendantElementIds(template: Template, groupId: string): Set<string> {
  const ids = new Set<string>();
  const visited = new Set<string>();
  const pending = [groupId];
  while (pending.length) {
    const current = pending.pop()!;
    if (visited.has(current)) continue;
    visited.add(current);
    const group = template.groups?.find((item) => item.id === current);
    if (!group) continue;
    group.elementIds.forEach((id) => ids.add(id));
    template.groups?.filter((item) => item.parentGroupId === current).forEach((child) => pending.push(child.id));
  }
  return ids;
}

export function descendantGroupIds(template: Template, groupId: string): Set<string> {
  const ids = new Set<string>();
  const pending = [groupId];
  const visited = new Set<string>([groupId]);
  while (pending.length) {
    const current = pending.pop()!;
    template.groups?.filter((item) => item.parentGroupId === current).forEach((child) => {
      if (!visited.has(child.id)) { ids.add(child.id); visited.add(child.id); pending.push(child.id); }
    });
  }
  return ids;
}

export function groupPath(template: Template, groupId: string | null): TemplateGroup[] {
  const path: TemplateGroup[] = [];
  let current = groupId;
  const seen = new Set<string>();
  while (current && !seen.has(current)) {
    seen.add(current);
    const group = template.groups?.find((item) => item.id === current);
    if (!group) break;
    path.unshift(group);
    current = group.parentGroupId;
  }
  return path;
}

export function resolveElementNode(template: Template, elementId: string, focusGroupId: string | null): DesignNode | null {
  let parent = parentOfNode(template, { kind: "element", id: elementId });
  if (parent === focusGroupId) return { kind: "element", id: elementId };
  const visited = new Set<string>();
  while (parent && !visited.has(parent)) {
    visited.add(parent);
    const group = template.groups?.find((item) => item.id === parent);
    if (!group) return null;
    if (group.parentGroupId === focusGroupId) return { kind: "group", id: group.id };
    parent = group.parentGroupId;
  }
  return null;
}

export function isElementInsideFocus(template: Template, elementId: string, focusGroupId: string | null): boolean {
  return focusGroupId === null || descendantElementIds(template, focusGroupId).has(elementId);
}

export function elementBox(element: TemplateElement) {
  if (element.type === "line") return {
    x: Math.min(element.x1, element.x2), y: Math.min(element.y1, element.y2),
    w: Math.max(Math.abs(element.x2 - element.x1), 0.8), h: Math.max(Math.abs(element.y2 - element.y1), 0.8),
  };
  if (element.type === "qr") return { x: element.x, y: element.y, w: element.size, h: element.size };
  return { x: element.x, y: element.y, w: element.w, h: element.h };
}

export function groupBox(template: Template, groupId: string) {
  const ids = descendantElementIds(template, groupId);
  const boxes = template.elements.filter((element) => ids.has(element.id)).map(elementBox);
  if (!boxes.length) return null;
  const x = Math.min(...boxes.map((box) => box.x));
  const y = Math.min(...boxes.map((box) => box.y));
  const right = Math.max(...boxes.map((box) => box.x + box.w));
  const bottom = Math.max(...boxes.map((box) => box.y + box.h));
  return { x, y, w: right - x, h: bottom - y };
}

export function validateGroups(template: Template): string | null {
  const groups = template.groups ?? [];
  const ids = new Set<string>();
  const elementIds = new Set<string>();
  for (let index = 0; index < template.elements.length; index++) {
    const id = template.elements[index].id;
    if (!id?.trim() && groups.length) return `Element #${index + 1} has a missing ID`;
    if (!id?.trim()) continue; // Keep rendering legacy, group-free templates with generated backend IDs.
    if (ids.has(id)) return `Duplicate template ID “${id}”`;
    ids.add(id);
    elementIds.add(id);
  }
  const groupById = new Map<string, TemplateGroup>();
  for (let index = 0; index < groups.length; index++) {
    const group = groups[index];
    if (!group.id?.trim()) return `Group #${index + 1} has a missing ID`;
    if (ids.has(group.id)) return `Duplicate template ID “${group.id}”`;
    ids.add(group.id);
    if (!group.name || group.name.trim() !== group.name || group.name.length < 1 || group.name.length > 60)
      return `Group “${group.id}” must have a trimmed name of 1–60 characters`;
    groupById.set(group.id, group);
  }
  const assigned = new Set<string>();
  for (const group of groups) {
    if (group.parentGroupId !== null && !groupById.has(group.parentGroupId))
      return `Group “${group.name}” refers to missing parent group “${group.parentGroupId}”`;
    for (const id of group.elementIds) {
      if (!elementIds.has(id)) return `Group “${group.name}” refers to missing element “${id}”`;
      if (assigned.has(id)) return `Element “${id}” belongs directly to more than one group`;
      assigned.add(id);
    }
    const seen = new Set<string>();
    let current: string | null = group.id;
    while (current) {
      if (seen.has(current)) return `Group cycle detected at “${group.name}”`;
      seen.add(current);
      current = groupById.get(current)?.parentGroupId ?? null;
    }
  }
  for (const group of groups) {
    const descendants = descendantElementIds(template, group.id);
    if (!descendants.size) return `Group “${group.name}” is empty and must be removed`;
    if (template.layout?.mode === "list") {
      const bands = new Set(template.elements.filter((element) => descendants.has(element.id)).map((element) => bandOf(element, template)));
      if (bands.size > 1) return `Group “${group.name}” contains elements from different list bands (${[...bands].join(", ")})`;
    }
  }
  return null;
}

function bandOf(element: TemplateElement, template: Template): "header" | "row" | "footer" {
  const layout = template.layout;
  if (layout?.mode !== "list") return "row";
  const top = element.type === "line" ? Math.min(element.y1, element.y2) : element.y;
  return top < layout.header_mm - 0.001 ? "header" : top < layout.header_mm + layout.row_mm - 0.001 ? "row" : "footer";
}
