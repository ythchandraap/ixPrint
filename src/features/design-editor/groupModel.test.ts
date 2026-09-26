import { describe, expect, it } from "vitest";
import type { Template } from "@/shared/api";
import { descendantElementIds, groupPath, resolveElementNode, validateGroups } from "./groupModel";

const base = (): Template => ({
  width_mm: 60,
  height_mm: 40,
  elements: [
    { type: "rect", id: "e1", x: 2, y: 2, w: 10, h: 5, stroke: 1, fill: false },
    { type: "rect", id: "e2", x: 20, y: 2, w: 10, h: 5, stroke: 1, fill: false },
    { type: "rect", id: "e3", x: 2, y: 10, w: 10, h: 5, stroke: 1, fill: false },
  ],
  groups: [
    { id: "ga", name: "Group A", parentGroupId: null, elementIds: ["e3"] },
    { id: "gb", name: "Group B", parentGroupId: "ga", elementIds: ["e1", "e2"] },
  ],
  sample: {},
});

describe("group model", () => {
  it("resolves the visible direct node at each focus level", () => {
    const template = base();
    expect(resolveElementNode(template, "e1", null)).toEqual({ kind: "group", id: "ga" });
    expect(resolveElementNode(template, "e1", "ga")).toEqual({ kind: "group", id: "gb" });
    expect(resolveElementNode(template, "e1", "gb")).toEqual({ kind: "element", id: "e1" });
    expect(resolveElementNode(template, "e3", "ga")).toEqual({ kind: "element", id: "e3" });
    expect(groupPath(template, "gb").map((group) => group.id)).toEqual(["ga", "gb"]);
    expect([...descendantElementIds(template, "ga")].sort()).toEqual(["e1", "e2", "e3"]);
  });

  it("accepts a valid nested tree without changing flat render order or positions", () => {
    const template = base();
    expect(validateGroups(template)).toBeNull();
    expect(template.elements.map((element) => element.id)).toEqual(["e1", "e2", "e3"]);
    expect(template.elements[0]).toMatchObject({ x: 2, y: 2 });
  });

  it.each([
    ["cycle", (t: Template) => { t.groups![0].parentGroupId = "gb"; }],
    ["missing parent", (t: Template) => { t.groups![1].parentGroupId = "absent"; }],
    ["missing element", (t: Template) => { t.groups![0].elementIds.push("absent"); }],
    ["multiple membership", (t: Template) => { t.groups![0].elementIds.push("e1"); }],
    ["duplicate ID", (t: Template) => { t.groups![0].id = "e1"; }],
    ["empty group", (t: Template) => { t.groups![1].elementIds = []; t.groups![0].elementIds = []; }],
  ])("rejects invalid %s metadata", (_name, corrupt) => {
    const template = base();
    corrupt(template);
    expect(validateGroups(template)).toBeTruthy();
  });

  it("rejects groups whose descendants cross list bands", () => {
    const template = base();
    template.layout = { mode: "list", header_mm: 5, row_mm: 10, footer_mm: 5, footer: "last_page", roll: false };
    expect(validateGroups(template)).toContain("Group “Group A” contains elements from different list bands");
  });
});
