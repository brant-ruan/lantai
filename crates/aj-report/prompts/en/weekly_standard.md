Using the material below, write one project's section of the weekly report for {period}. The material is this project's sessions with coding agents during the week, day by day; each day lists the condensed slices (Goal, Key changes, …), and busy days have already been condensed into that day's task clusters.

First group the week's material by task, then write. The weekly report is for quick reading, so write less rather than piling up detail:
- The number of threads follows the amount of work: 1 or 2 threads for a project with only one or two sessions, 3 to 5 for a project with a lot of work. Work on the same thing across days (the same session continuing over days, or different sessions or agents advancing the same thing) must be one thread; the steps toward one goal (implementing, debugging, testing, releasing) belong to one thread, do not split them; merge small odds and ends into a final "Other" thread, or leave them out
- Write each thread as one top-level point: `- one sentence on the task itself and this week's main result` (at most about 20 words, not bold, no dates)
- The indented points tell how it progressed, in time order: which day it started, the key milestones, and where the week's last related session left it. Start each indented point with the date (like "11-17: "), merge neighbouring days of similar progress into one point (like "11-17–11-19: "), and include the source anchors
- Each indented point covers one thing in one sentence of at most about 25 words (not counting the date and anchors); keep only the conclusion or number that best shows progress, and leave out parameters, file names, commands, commit hashes and itemized lists; readers who want detail will follow the anchors
- 2 to 4 indented points per thread, 1 or 2 for a thread with only one or two sessions; pick the key milestones rather than listing every slice

Format:
- Markdown, with the top-level heading "# {period}" (the program rewrites it)
- Exactly one second-level heading "## <project name>"; the project name must match the project name in the material exactly
- Use only "- " bullet lists and sub-lists indented by 2 spaces, no loose paragraphs; do not write statistics (active time and session counts are filled in by the program)
- "Same-session hints" lists slices of the same session on different days; they usually belong to one thread
