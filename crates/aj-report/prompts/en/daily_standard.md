Using the material below, write the work log for {date}. The material comes from that day's sessions with coding agents, grouped by project and condensed.

Format:
- Markdown, with the top-level heading "# {date}" (the program rewrites it as "date · weekday")
- One second-level heading "## <project name>" per project; the project name must match the project group name in the material exactly. Do not add second-level headings that are not projects at the end
- Use a multi-level outline of bullet lists, "top-level conclusions + collapsible details" (strictly 2 spaces of indentation per level):
  - **Top-level points (L2)**: `- one sentence on the core task and its result` (at most about 15 words, not bold), summarizing only "what was done / what changed / what was concluded", for a reader with limited attention; **strictly 3 to 5 per project** (1 or 2 for small changes; when there would be more, merge themes across sessions)
  - **Indented points (L3)**: `  - concrete approach, key change, technical trade-off or investigation [#S12.T5]`, carrying the technical details from the follow-up turns and the source anchors
  - **One more level (L4)**: not used by default; almost every top-level point stops at L3. L4 (4 spaces) is allowed in one case only: a top-level point has more than 4 details that naturally fall into at least 2 sub-topics with at least 2 details each. Then L3 names the sub-topic (a short phrase, no anchor) and L4 holds the details with anchors. When there are many details but no clear sub-topics, trim and merge them instead of adding a level
  - Otherwise do not nest to L4; do not group just to look organized, and never group a single detail under a sub-topic; never use a fifth level

Content:
- Top-level points reflect the user's core goals and results; do not put steps, commands or scattered file names in them
- When several sessions in one project work on the same thing, merge them into one top-level point and list each session's progress as indented points
- Write pure exploration or Q&A sessions plainly as research or discussion conclusions
