import { describe, expect, it } from "vitest";
import { assignableProjects } from "./projectOptions";

describe("assignableProjects", () => {
  const projects = [
    { id: 1, name: "Active" },
    { id: 2, name: "Archived", archived: true },
    { id: 3, name: "Also active", archived: false },
  ];

  it("leaves archived projects out", () => {
    expect(assignableProjects(projects).map((p) => p.id)).toEqual([1, 3]);
    expect(assignableProjects(projects, null).map((p) => p.id)).toEqual([1, 3]);
  });

  it("keeps an archived project that is already chosen, in its place", () => {
    expect(assignableProjects(projects, 2).map((p) => p.id)).toEqual([1, 2, 3]);
  });
});
