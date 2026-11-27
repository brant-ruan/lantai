# Website

What to know when maintaining the project site at `https://lantai.wohin.me`. Its source is
`website/`: plain HTML, CSS and a little JavaScript, no framework and no build step.

## Audience and goals

People who hand work to agents (Claude Code, Codex CLI, Antigravity and others) and want a record
of it, mostly developers. The page has three jobs, in order:

1. **Say what it is within ten seconds:** a work journal written by you and your agents. Agents
   draft daily, weekly and monthly reports from local sessions; you write what the sessions do not
   contain; data stays on the machine.
2. **Answer privacy concerns** in a section of their own: read-only sources, redaction before
   storage and before sending, keys in the keychain, no account and no telemetry.
3. **Get people started:** download, first open, CLI, building from source.

**The story:** *Your agents keep the record. You keep the meaning.* Work has become conversation;
the record can be automated, the meaning cannot; one journal, two hands; and this is an attempt at
how people and agents can work together, not an answer. "Respecting the person" is shown through
mechanisms that exist (human writing stored apart and never touched by regeneration, edit marks
and the source filter, reports that describe without judging, assignments confirmed by the user),
not slogans. The page does not go into details such as which notes become report material.

**Copy:** short, concrete, no hype ("AI-powered", "revolutionary"). Say "agents", not "coding
agents"; name products when an example helps.

## Structure

- English at `/` (`website/index.html`), Chinese at `/zh/` (`website/zh/index.html`). **The two
  pages correspond section by section**; the Chinese text is rewritten for Chinese readers, not
  translated line by line. Change both together. Each page links to the other in its header and
  through `hreflang` (`x-default` is English).
- Sections, top to bottom: header (sticky, anchors, language switch, GitHub); hero (headline,
  buttons, the interactive demo, and the roll-up drawing); the problem (a day scattered over a dozen
  sessions, with the most important part in none of them); two hands (what agents write, what you
  write, with the annotated-page drawing); how it works (gather → draft → write); feature cards;
  supported agents (a scrolling strip of names and icons); privacy (dark band); get started; why
  it exists (same text as the About page); the name (the story of 兰台); footer (GitHub, docs,
  licenses, trademark notice).
- The README's text matches the site; keep them in step.

## Visual rules

- Matches the desktop app's default GitHub Light theme (GitHub Dark in dark mode): white
  background, blue accent (`#0969DA` / `#4493F8`), system sans-serif fonts, system monospace for
  code, system serif only for 兰台 in the name section. No web fonts.
- **Purple means the person.** The human color (`#8250DF` / `#AB7DF8`, the app's `--human`) is
  used only for human writing: in the illustrations, the demo and the roll-up drawing.
- Content up to 1120 px wide, paragraphs up to 680 px; 16 px side gutters on phones and no
  horizontal scrolling. Thin rules instead of card shadows. Motion: fade-in on scroll and drawing
  animations, all off under `prefers-reduced-motion`. Follows `prefers-color-scheme`, with a
  manual switch.
- **No orphans:** body text uses `text-wrap: pretty`, short lead paragraphs `balance`, and
  `site.js` keeps the last five characters of Chinese paragraphs together. Copy is tuned so side-by-
  side cards have equal line counts at desktop widths; recheck at 1440 px after changing text.
- **Illustrations** draw neither people nor robots. Agents and people differ only by hand: agent
  writing is blue, monospace and geometric and appears line by line; human writing is purple and
  hand-drawn and appears as if written with a pen. `website/assets/two-hands.svg` (the annotated
  page) animates with CSS inside the SVG when `site.js` adds `.in`; the README uses static copies
  (`docs/images/two-hands-{light,dark}.svg`).
- **Agent icons** in `website/assets/agents/` come from the LobeHub icon set
  (`@lobehub/icons-static-svg`, colored versions, MIT, license in the folder), with `width`,
  `height` and `style` removed, used as `<img>`. A new agent needs its icon and its name in the
  footer's trademark notice.

## The roll-up drawing

`website/assets/rollup.js` (styles in `site.css`) draws scattered sessions flowing into days, weeks
and a month, narrowing and converging from four hues to the accent color, with a few purple points
for human notes joining by dashed lines. It stays abstract: no text, numbers or tooltips; data is
a fixed-seed made-up month. It draws in when visible, then sparks travel up the lines; hovering a
node highlights its branch. It pauses off screen and redraws on resize.

## The interactive demo

The hero demo imitates the desktop app's layout with made-up data and can be clicked through.

- Files: `assets/demo-data.js` (data), `assets/demo-data.zh.js` (loaded by the Chinese page to
  replace text), `assets/demo.js` (derivation and rendering), `assets/demo.css` (layout sizes copied
  from `app/src/styles/`, colors from the GitHub themes).
- Four fictional projects and five agents; a `schedule` says what happened each day, and sessions,
  turns, daily and weekly reports are derived from it; the monthly report is hand-written. Human
  notes are in `notes`. The calendar is fixed to one month.
- It supports the views, tabs, toolbar and source filter, folding, anchors opening the drawer,
  reassigning and ignoring followed by Update, generation progress, draft / final, editing, the
  project page, statistics, and sidebar report batches in all three period lists. Version
  history, export and settings do nothing.
- Interface text is English on both pages, as in the app; report and session content is Chinese
  on the Chinese page.
- **It does not share code with `app/`.** When the desktop interface changes noticeably, update
  the demo by hand, and retake the README screenshots (see `AGENTS.md`).

## Screenshots and privacy

Real screens contain real sessions and project names and must never be captured. Screenshots come
only from the demo (or from a separate data directory filled with made-up sessions through
`AJ_DATA_DIR` and the `AJ_*_ROOT` variables); check every image for real paths, user names and
repository addresses before committing it.

## Technical

- One HTML file per language; no runtime translation.
- No analytics or tracking of any kind.
- SEO: `<title>`, description, Open Graph and Twitter cards, `hreflang`, `sitemap.xml`,
  `robots.txt`.
- Local preview: the `website` configuration in `.claude/launch.json`
  (`python3 -m http.server 4173 --directory website`).

## Deployment

`website/` is deployed to GitHub Pages by `.github/workflows/pages.yml` on every push to `main`
that touches it (or by hand). The `build` job packages and uploads the static files, then
`deploy` waits for `build` before publishing. Keeping them in separate jobs avoids an
[intermittent artifact lookup failure](https://github.com/actions/deploy-pages/issues/451)
when deployment starts immediately after upload in the same job.

Repository settings it relies on:

1. Settings → Pages → Source: **GitHub Actions**.
2. Custom domain `lantai.wohin.me` (with Actions deployment the `CNAME` file is not used) and
   **Enforce HTTPS**. DNS: `lantai` CNAME `brant-ruan.github.io`.
3. Optionally, verify the domain in the account's Pages settings to prevent takeover of
   subdomains.
