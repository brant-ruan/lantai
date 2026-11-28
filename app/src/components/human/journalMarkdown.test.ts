import { describe, expect, it } from "vitest";
import { periodMarkdown, rollupMarkdown } from "./journalMarkdown";
import { journalOf, node } from "./testing";
import { journal, labels, projects, report } from "./fixture";

const lines = (...l: string[]) => l.join("\n");

describe("periodMarkdown", () => {
  const copy = (filter: "both" | "agent" | "human") =>
    periodMarkdown({ markdown: report.join("\n"), journal, projects, filter, labels });

  it("copies the agent's text alone with a blank line after each heading", () => {
    expect(
      periodMarkdown({
        markdown: "# T\n## A\n- x",
        journal,
        projects,
        filter: "agent",
        labels,
      }),
    ).toBe("# T\n\n## A\n\n- x");
  });

  it("quotes what people wrote among the agent's points", () => {
    expect(copy("both")).toBe(
      lines(
        "# 2026-11-16 · Mon",
        "",
        "## Day notes",
        "",
        "> - day note",
        "",
        "## **Alpha**",
        "",
        "> - before alpha",
        "",
        "- did a thing [#S1]",
        "  > - remark",
        "  - detail",
        "",
        "> - after alpha",
        ">   - nested",
        ">     second line",
        "",
        "## Beta",
        "",
        "- other thing",
        "",
        "> *Point changed*: old point",
        ">",
        "> - stale",
        "",
        "## Misc",
        "",
        "- misc",
        "",
        "## Gamma",
        "",
        "> - gamma only",
        "",
        "## Other annotations",
        "",
        "> *Point changed*",
        ">",
        "> - lost",
      ),
    );
  });

  it("copies only what people wrote, quoting the points as context", () => {
    expect(copy("human")).toBe(
      lines(
        "# 2026-11-16 · Mon",
        "",
        "## Day notes",
        "",
        "- day note",
        "",
        "## Alpha",
        "",
        "- before alpha",
        "",
        "> did a thing",
        "",
        "- remark",
        "",
        "- after alpha",
        "  - nested",
        "    second line",
        "",
        "## Beta",
        "",
        "> *Point changed*: old point",
        "",
        "- stale",
        "",
        "## Gamma",
        "",
        "- gamma only",
        "",
        "## Other annotations",
        "",
        "> *Point changed*",
        "",
        "- lost",
      ),
    );
  });

  it("copies the notes of a day without a report", () => {
    expect(
      periodMarkdown({ markdown: "", journal, projects, filter: "human", labels }),
    ).toContain("## Day notes\n\n- day note");
  });
});

describe("rollupMarkdown", () => {
  const page = node({ body: "page note", periodKind: "project", periodKey: "", projectId: 1 });
  const morning = node({ body: "morning", projectId: 1, position: "top" });
  const remark = node({
    body: "remark",
    projectId: 1,
    isAnnotation: true,
    quote: "did a thing",
  });
  const evening = node({ body: "evening", projectId: 1, periodKey: "2026-11-17" });
  const j = journalOf([page, morning, remark, evening], [
    { id: remark.id, kind: "point", line: 0 },
  ]);
  const copy = (filter: "both" | "agent" | "human") =>
    rollupMarkdown({
      title: "# Alpha",
      entries: [
        { period: "2026-11-16", label: "Mon 16", markdown: "- did a thing\n  - detail" },
        { period: "2026-11-17", label: "Tue 17", markdown: "" },
      ],
      periodKind: "day",
      journal: j,
      filter,
      labels,
    });

  it("copies entries with notes quoted among the points", () => {
    expect(copy("both")).toBe(
      lines(
        "# Alpha",
        "",
        "## Page notes",
        "",
        "> - page note",
        "",
        "## Mon 16",
        "",
        "> - morning",
        "",
        "- did a thing",
        "  > - remark",
        "  - detail",
        "",
        "## Tue 17",
        "",
        "> - evening",
      ),
    );
  });

  it("leaves out notes and entries without text when copying the agent's text", () => {
    expect(copy("agent")).toBe("# Alpha\n\n## Mon 16\n\n- did a thing\n  - detail");
  });

  it("copies only notes, with an annotation's point as context", () => {
    expect(copy("human")).toBe(
      lines(
        "# Alpha",
        "",
        "## Page notes",
        "",
        "- page note",
        "",
        "## Mon 16",
        "",
        "- morning",
        "",
        "> did a thing",
        "",
        "- remark",
        "",
        "## Tue 17",
        "",
        "- evening",
      ),
    );
  });
});
