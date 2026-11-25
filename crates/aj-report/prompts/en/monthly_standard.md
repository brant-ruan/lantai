Using the material below, write one project's section of the monthly report for {period}. The material is this project's sections from each weekly report of the month, week by week.

First merge the month's material by task across weeks, then write. The monthly report is for skimming the whole month, so do not list it week by week, and write less rather than piling up detail:
- The number of threads follows the amount of work: 1 or 2 threads for a project with little work this month, 3 to 5 for a project with a lot. Work on the same thing across weeks must be one thread; the stages toward one goal (design, implementation, debugging, testing, polishing) belong to one thread, do not split them
- Write each thread as one top-level point: `- one sentence on the task itself and this month's main result` (at most about 20 words, not bold, no weeks or dates)
- The indented points tell how it progressed across weeks, in time order: which week (or day) it started, the key milestones, and where it stood at the end of the month. Start each indented point with the week or date range (like "W45: " or "11-03–11-12: ") and keep the source anchors from the weekly reports (like `[#S12.T5]`)
- Each indented point covers one thing in one sentence of at most about 25 words (not counting the time prefix and anchors); keep only the conclusion or number that best shows progress, and leave out parameters, file names, commands, commit hashes and itemized lists
- 2 to 4 indented points per thread, 1 or 2 for a small thread; pick the key milestones rather than copying every week's points
- Fit minor work into a related thread where it belongs, and omit items without a distinct outcome worth revisiting. If one standalone outcome is worth revisiting, name it directly in a final top-level point and keep its source anchors from the weekly reports. If two or more are worth revisiting, group them under a final top-level point named for their shared theme, or use “Other progress” if they have no shared theme. Give each outcome its own indented point using the date, length and source-anchor rules above. Never write a count-only point

Format:
- Markdown, with the top-level heading "# {period}" (the program rewrites it)
- Exactly one second-level heading "## <project name>"; the project name must match the project name in the material exactly
- Use only "- " bullet lists and sub-lists indented by 2 spaces, no loose paragraphs; do not write statistics (active days, time and session counts are filled in by the program)
