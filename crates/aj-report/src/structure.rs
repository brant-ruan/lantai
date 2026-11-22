//! Structural validation, normalization, and per-project extraction for reports.
//!
//! Reports (daily and weekly) must follow a predictable outline (`# Title`, one `## <Project>`
//! heading per project, and `- ` bullet lists inside each section) so that Project
//! Views can deterministically extract and roll up project entries across days.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::lang::Language;
use crate::material::UNASSIGNED;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSection {
    pub project: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedReport {
    pub title: String,
    pub sections: Vec<ProjectSection>,
    pub markdown: String,
}

/// The canonical title of a report of `kind` for `period_key`, written in `lang`.
pub fn report_title(kind: &str, period_key: &str, lang: &Language) -> String {
    match kind {
        "daily" => lang.daily_title(period_key),
        "weekly" => match crate::period::Week::parse(period_key) {
            Ok(w) => w.title(),
            Err(_) => format!("# {period_key}"),
        },
        "monthly" => match crate::period::Month::parse(period_key) {
            Ok(m) => m.title(),
            Err(_) => format!("# {period_key}"),
        },
        _ => format!("# {period_key}"),
    }
}

/// Replace the report's H1 (the first `# ` line before any section) with `title`, or
/// insert it when missing. Other lines are untouched.
pub fn with_title(markdown: &str, title: &str) -> String {
    let mut out = String::with_capacity(markdown.len() + 16);
    let mut done = false;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim();
        if !done {
            if trimmed.starts_with("# ") {
                let eol = &line[line.trim_end_matches(['\n', '\r']).len()..];
                out.push_str(title);
                out.push_str(eol);
                done = true;
                continue;
            }
            if trimmed.starts_with("## ") || (!trimmed.is_empty() && !trimmed.starts_with('#')) {
                out.push_str(title);
                out.push_str("\n\n");
                done = true;
            }
        }
        out.push_str(line);
    }
    if !done {
        out.push_str(title);
    }
    out
}

/// Strip surrounding formatting noise and leading labels from a heading before matching.
pub(crate) fn strip_heading_wrappers(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    loop {
        let before = s.clone();
        s = s.trim().to_string();
        for (open, close) in [
            ("**", "**"),
            ("__", "__"),
            ("`", "`"),
            ("\"", "\""),
            ("'", "'"),
            ("[", "]"),
            ("\u{3010}", "\u{3011}"),
        ] {
            if s.starts_with(open) && s.ends_with(close) && s.len() > open.len() + close.len() {
                s = s[open.len()..s.len() - close.len()].trim().to_string();
            }
        }
        // Strip leading numbering like "1. " or "2) ".
        if let Some(pos) = s.find(". ").or_else(|| s.find(") ")) {
            if pos > 0 && s[..pos].chars().all(|c| c.is_ascii_digit()) {
                s = s[pos + 2..].trim().to_string();
            }
        }
        // Strip leading "Project:" / "Project：".
        let lower = s.to_ascii_lowercase();
        if let Some(rest) = lower
            .strip_prefix("project:")
            .or_else(|| lower.strip_prefix("project :"))
        {
            let offset = s.len() - rest.len();
            s = s[offset..].trim().to_string();
        } else if let Some(rest) = s.strip_prefix("project\u{ff1a}") {
            s = rest.trim().to_string();
        } else if let Some(rest) = s.strip_prefix("Project\u{ff1a}") {
            s = rest.trim().to_string();
        }
        if s == before {
            break;
        }
    }
    s
}

/// Match an H2 heading against allowed project names, returning the canonical project name.
fn resolve_heading<'a>(raw: &str, allowed: &'a [String]) -> Option<&'a str> {
    let cleaned = strip_heading_wrappers(raw);
    if cleaned.is_empty() {
        return None;
    }
    let lowered = cleaned.to_lowercase();
    if let Some(found) = allowed.iter().find(|p| p.to_lowercase() == lowered) {
        return Some(found.as_str());
    }
    if matches!(lowered.as_str(), "other" | "misc" | "unassigned") {
        if let Some(found) = allowed.iter().find(|p| p.eq_ignore_ascii_case(UNASSIGNED)) {
            return Some(found.as_str());
        }
    }
    None
}

fn is_bold_label(trimmed: &str) -> bool {
    let stripped = trimmed.trim_end_matches([':', '\u{ff1a}']).trim();
    stripped.starts_with("**")
        && stripped.ends_with("**")
        && stripped.len() > 4
        && !stripped[2..stripped.len() - 2].contains("**")
}

/// Returns `Some((normalized_indent, content_after_bullet))` if `line` is an unordered list item.
fn parse_bullet_line(line: &str) -> Option<(String, &str)> {
    let indent_bytes = line.len() - line.trim_start_matches([' ', '\t']).len();
    let raw_indent = &line[..indent_bytes];
    let rest = &line[indent_bytes..];
    let after_marker = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("\u{2022} "))?;
    let mut normalized_indent = String::with_capacity(indent_bytes);
    for ch in raw_indent.chars() {
        if ch == '\t' {
            normalized_indent.push_str("  ");
        } else {
            normalized_indent.push(' ');
        }
    }
    Some((normalized_indent, after_marker.trim_end()))
}

fn clip(s: &str, max_chars: usize) -> String {
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        out.push_str("...");
    }
    out
}

fn strip_code_fence(text: &str) -> &str {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return t;
    };
    let Some(body) = rest.strip_suffix("```") else {
        return t;
    };
    match body.find('\n') {
        Some(pos) => body[pos + 1..].trim(),
        None => t,
    }
}

fn validate_impl(
    raw: &str,
    title: &str,
    allowed_projects: &[String],
    require_all: bool,
) -> Result<ValidatedReport, Vec<String>> {
    let text = strip_code_fence(raw).replace("\r\n", "\n");
    let mut errors: Vec<String> = Vec::new();
    let mut seen_title = false;

    // Collect sections in first-appearance order while merging duplicate project headings.
    let mut order: Vec<String> = Vec::new();
    let mut section_lines: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut bullet_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut current: Option<String> = None;

    for (idx, raw_line) in text.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw_line.trim();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("# ") {
            if current.is_none() && !seen_title {
                // The model's own title is replaced by the canonical one below.
                seen_title = true;
                continue;
            }
            errors.push(format!(
                "line {line_no}: unexpected top-level H1 heading inside report body"
            ));
            continue;
        }

        if let Some(h2) = trimmed.strip_prefix("## ") {
            if !trimmed.starts_with("### ") {
                match resolve_heading(h2, allowed_projects) {
                    Some(canonical) => {
                        let key = canonical.to_string();
                        if !section_lines.contains_key(&key) {
                            order.push(key.clone());
                            section_lines.insert(key.clone(), Vec::new());
                            bullet_counts.insert(key.clone(), 0);
                        }
                        current = Some(key);
                    }
                    None => {
                        errors.push(format!(
                            "line {line_no}: unknown section \"## {}\"; expected project headings: {}",
                            h2.trim(),
                            allowed_projects.join(", ")
                        ));
                        current = None;
                    }
                }
                continue;
            }
        }

        let Some(ref proj) = current else {
            // Skip horizontal rules outside sections cleanly.
            if trimmed != "---" {
                errors.push(format!(
                    "line {line_no}: content outside any \"## <Project>\" section: \"{}\"",
                    clip(trimmed, 40)
                ));
            }
            continue;
        };

        if trimmed == "---" {
            continue;
        }

        let buf = section_lines.entry(proj.clone()).or_default();
        if let Some((indent, item)) = parse_bullet_line(raw_line) {
            if item.is_empty() {
                errors.push(format!(
                    "line {line_no}: empty bullet item in \"## {proj}\""
                ));
                continue;
            }
            buf.push(format!("{indent}- {item}"));
            *bullet_counts.entry(proj.clone()).or_default() += 1;
        } else if trimmed.starts_with("### ") || trimmed.starts_with("#### ") {
            if buf.last().is_some_and(|l| !l.is_empty()) {
                buf.push(String::new());
            }
            buf.push(trimmed.to_string());
        } else if trimmed.starts_with("> ") && *bullet_counts.get(proj).unwrap_or(&0) == 0 {
            // A statistics line written by the program, before the first bullet.
            buf.push(trimmed.to_string());
        } else if is_bold_label(trimmed) {
            if buf.last().is_some_and(|l| !l.is_empty()) {
                buf.push(String::new());
            }
            buf.push(trimmed.to_string());
        } else {
            errors.push(format!(
                "line {line_no}: section \"## {proj}\" has non-list text \"{}\"; use \"- \" bullet items",
                clip(trimmed, 40)
            ));
        }
    }

    if require_all {
        for expected in allowed_projects {
            if !section_lines.contains_key(expected) {
                errors.push(format!("missing required section \"## {expected}\""));
            }
        }
    }

    if order.is_empty() {
        errors.push("report contains no valid \"## <Project>\" sections".into());
    }

    let mut sections = Vec::with_capacity(order.len());
    for proj in order {
        let count = bullet_counts.get(&proj).copied().unwrap_or(0);
        if count == 0 {
            errors.push(format!(
                "section \"## {proj}\" must contain at least one \"- \" bullet point"
            ));
        }
        let mut lines = section_lines.remove(&proj).unwrap_or_default();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        while lines.first().is_some_and(|l| l.is_empty()) {
            lines.remove(0);
        }
        sections.push(ProjectSection {
            project: proj,
            body: lines.join("\n"),
        });
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(ValidatedReport {
        title: title.to_string(),
        markdown: compose_report(title, &sections),
        sections,
    })
}

/// A report from its sections: the title, then one `## <project>` per section.
pub fn compose_report(title: &str, sections: &[ProjectSection]) -> String {
    let mut markdown = String::new();
    let _ = writeln!(markdown, "{title}");
    for sec in sections {
        let _ = write!(markdown, "\n## {}\n{}\n", sec.project, sec.body.trim());
    }
    markdown.trim_end().to_string()
}

/// Apply an incremental update to a report's sections: sections named in `replacements`
/// take the new body in place, sections named in `removed` are dropped, and replacements
/// for projects the report did not have yet are appended. Other sections are kept as they
/// are. Names match case-insensitively.
pub fn splice_sections(
    base: &[ProjectSection],
    replacements: &[ProjectSection],
    removed: &[String],
) -> Vec<ProjectSection> {
    let same = |a: &str, b: &str| a.to_lowercase() == b.to_lowercase();
    let mut out: Vec<ProjectSection> = base
        .iter()
        .filter(|s| !removed.iter().any(|r| same(r, &s.project)))
        .map(|s| {
            replacements
                .iter()
                .find(|r| same(&r.project, &s.project))
                .cloned()
                .unwrap_or_else(|| s.clone())
        })
        .collect();
    for r in replacements {
        if !out.iter().any(|s| same(&s.project, &r.project)) {
            out.push(r.clone());
        }
    }
    out
}

/// Validate and normalize an LLM-generated daily report against the day's expected project headings.
pub fn validate_and_normalize(
    raw: &str,
    title: &str,
    expected_projects: &[String],
) -> Result<ValidatedReport, Vec<String>> {
    validate_impl(raw, title, expected_projects, true)
}

/// Validate and normalize a manually edited daily report against all known project names (plus Unassigned).
pub fn validate_user_edit(
    raw: &str,
    title: &str,
    known_projects: &[String],
) -> Result<ValidatedReport, Vec<String>> {
    let mut allowed = known_projects.to_vec();
    if !allowed.iter().any(|p| p.eq_ignore_ascii_case(UNASSIGNED)) {
        allowed.push(UNASSIGNED.to_string());
    }
    validate_impl(raw, title, &allowed, false)
}

/// Extract a single project's section body from a stored daily report Markdown.
pub fn extract_project_section(markdown: &str, project_name: &str) -> Option<String> {
    let allowed = [project_name.to_string()];
    let normalized = markdown.replace("\r\n", "\n");
    let mut inside = false;
    let mut collected: Vec<&str> = Vec::new();

    for line in normalized.lines() {
        let trimmed = line.trim();
        if let Some(h2) = trimmed.strip_prefix("## ") {
            if !trimmed.starts_with("### ") {
                if resolve_heading(h2, &allowed).is_some() {
                    inside = true;
                    continue;
                } else if inside {
                    break;
                }
            }
        }
        if inside {
            if trimmed.is_empty() {
                continue;
            }
            collected.push(line.trim_end());
        }
    }

    let body = collected.join("\n").trim().to_string();
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

/// Rewrite the `## <project>` headings that name `old_name` (matched exactly as
/// [`extract_project_section`] matches them) to `## <new_name>`, keeping line
/// endings and everything else untouched. Returns `None` when nothing changed.
pub fn rename_project_heading(markdown: &str, old_name: &str, new_name: &str) -> Option<String> {
    let allowed = [old_name.to_string()];
    let mut changed = false;
    let mut out = String::with_capacity(markdown.len() + 16);
    for line in markdown.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let eol = &line[body.len()..];
        let trimmed = body.trim();
        let renamed = trimmed
            .strip_prefix("## ")
            .filter(|_| !trimmed.starts_with("### "))
            .filter(|h2| resolve_heading(h2, &allowed).is_some())
            .map(|_| {
                let indent = &body[..body.len() - body.trim_start().len()];
                format!("{indent}## {new_name}{eol}")
            });
        match renamed {
            Some(r) if r != line => {
                changed = true;
                out.push_str(&r);
            }
            _ => out.push_str(line),
        }
    }
    changed.then_some(out)
}

/// Reorder the `## <project>` sections of a daily report to follow `order` (the
/// user's project order). Headings are matched as in [`extract_project_section`];
/// sections naming no listed project keep their relative order after the listed
/// ones. Text before the first section stays first. Returns the input unchanged
/// when it is already in order.
pub fn order_project_sections(markdown: &str, order: &[String]) -> String {
    let mut preamble = String::new();
    let mut sections: Vec<(usize, String)> = Vec::new();
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim();
        let h2 = trimmed
            .strip_prefix("## ")
            .filter(|_| !trimmed.starts_with("### "));
        if let Some(h2) = h2 {
            let rank = resolve_heading(h2, order)
                .and_then(|name| order.iter().position(|o| o == name))
                .unwrap_or(order.len());
            sections.push((rank, String::new()));
        }
        match sections.last_mut() {
            Some((_, text)) => text.push_str(line),
            None => preamble.push_str(line),
        }
    }
    if sections.windows(2).all(|w| w[0].0 <= w[1].0) {
        return markdown.to_string();
    }
    // The last section may lack a trailing newline; give every section one so
    // that moving it does not glue it to the next heading.
    let ends_with_newline = markdown.ends_with('\n');
    for (_, text) in &mut sections {
        if !text.ends_with('\n') {
            text.push('\n');
        }
    }
    sections.sort_by_key(|(rank, _)| *rank);
    let mut out = preamble;
    for (_, text) in sections {
        out.push_str(&text);
    }
    if !ends_with_newline {
        while out.ends_with('\n') {
            out.pop();
        }
    }
    out
}

/// Combine extracted daily report project sections into a single exportable Markdown document.
pub fn build_project_rollup(
    project_name: &str,
    period_label: &str,
    entries: &[(String, String)],
) -> String {
    let mut out = format!("# {project_name} · {period_label}\n");
    for (date, body) in entries {
        let trimmed = body.trim();
        if !trimmed.is_empty() {
            let _ = write!(out, "\n## {date}\n{trimmed}\n");
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod splice_tests {
    use super::*;

    fn sec(project: &str, body: &str) -> ProjectSection {
        ProjectSection {
            project: project.into(),
            body: body.into(),
        }
    }

    #[test]
    fn splice_replaces_in_place_drops_removed_and_appends_new() {
        let base = vec![sec("A", "- a"), sec("B", "- b edited"), sec("C", "- c")];
        let out = splice_sections(
            &base,
            &[sec("a", "- a2"), sec("D", "- d")],
            &["C".to_string()],
        );
        assert_eq!(
            out,
            vec![sec("a", "- a2"), sec("B", "- b edited"), sec("D", "- d")]
        );
        assert_eq!(
            compose_report(&daily_title("2026-11-16"), &out[1..2]),
            format!("{}\n\n## B\n- b edited", daily_title("2026-11-16"))
        );
    }
}

#[cfg(test)]
pub(crate) fn daily_title(date: &str) -> String {
    crate::lang::ZH_HANS.daily_title(date)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{EN, ZH_HANS};

    #[test]
    fn titles_follow_the_report_language() {
        assert_eq!(
            report_title("daily", "2026-11-08", &ZH_HANS),
            "# 2026-11-08 \u{00b7} \u{5468}\u{65e5}"
        );
        assert_eq!(
            report_title("daily", "2026-11-08", &EN),
            "# 2026-11-08 \u{00b7} Sun"
        );
        assert_eq!(
            report_title("weekly", "2026-W46", &EN),
            report_title("weekly", "2026-W46", &ZH_HANS)
        );
        let t = daily_title("2026-11-08");
        assert_eq!(
            with_title("# Old title\r\n\n## A\n- a\n", &t),
            format!("{t}\r\n\n## A\n- a\n")
        );
        assert_eq!(with_title("## A\n- a", &t), format!("{t}\n\n## A\n- a"));
    }

    #[test]
    fn orders_project_sections_by_user_order() {
        let order = vec!["Beta".to_string(), "Alpha".to_string()];
        let md = "# 2026-11-17 工作日志\n\n## Alpha\n- a\n  - detail\n\n## Other\n- o\n\n## **beta**\n- b";
        assert_eq!(
            order_project_sections(md, &order),
            "# 2026-11-17 工作日志\n\n## **beta**\n- b\n## Alpha\n- a\n  - detail\n\n## Other\n- o"
        );
        let sorted = "# T\n## Beta\n- b\n## Alpha\n- a\n";
        assert_eq!(order_project_sections(sorted, &order), sorted);
    }

    #[test]
    fn renames_only_matching_project_headings() {
        let md = "# 2026-11-17 工作日志\r\n\n## **Old Name**\n- did a [#S1.T2]\n\n## Other\n- b\n### Old Name\n## old name\n";
        let out = rename_project_heading(md, "Old Name", "New").unwrap();
        assert_eq!(
            out,
            "# 2026-11-17 工作日志\r\n\n## New\n- did a [#S1.T2]\n\n## Other\n- b\n### Old Name\n## New\n"
        );
        assert_eq!(
            extract_project_section(&out, "New").as_deref(),
            Some("- did a [#S1.T2]")
        );
        assert!(rename_project_heading(md, "Missing", "X").is_none());
        // A case-only rename of an already-exact heading still rewrites the differing line.
        assert!(rename_project_heading("## new\n", "New", "new").is_none());
    }

    #[test]
    fn validates_and_normalizes_clean_report_with_nested_outlines() {
        let raw = r#"```markdown
# Daily Report · 2026-11-16

## Project: **Atlas**
### Completed
* Fixed readiness checks [#S1.T0]
  - Added bounds validation [#S1.T1]

## unassigned
- Answered syntax question [#S2.T0]
```"#;
        let expected = vec!["Atlas".into(), "Unassigned".into()];
        let out = validate_and_normalize(raw, &daily_title("2026-11-16"), &expected).unwrap();

        assert_eq!(out.title, "# 2026-11-16 \u{00b7} \u{5468}\u{4e00}");
        assert_eq!(out.sections.len(), 2);
        assert_eq!(out.sections[0].project, "Atlas");
        assert_eq!(out.sections[1].project, "Unassigned");
        assert_eq!(
            out.markdown,
            "# 2026-11-16 \u{00b7} \u{5468}\u{4e00}\n\n## Atlas\n### Completed\n- Fixed readiness checks [#S1.T0]\n  - Added bounds validation [#S1.T1]\n\n## Unassigned\n- Answered syntax question [#S2.T0]"
        );
    }

    #[test]
    fn rejects_unknown_h2_headings_and_missing_projects_and_prose() {
        let expected = vec!["App".into(), "Docs".into()];
        let bad = r#"# Daily Report · 2026-11-16

## App
We refactored the auth module today without bullet points.

## Next Steps
- Deploy tomorrow
"#;
        let errs = validate_and_normalize(bad, &daily_title("2026-11-16"), &expected).unwrap_err();
        let joined = errs.join(" | ");
        assert!(joined.contains("non-list text"), "{joined}");
        assert!(
            joined.contains("unknown section \"## Next Steps\""),
            "{joined}"
        );
        assert!(
            joined.contains("missing required section \"## Docs\""),
            "{joined}"
        );
        assert!(joined.contains("must contain at least one"), "{joined}");
    }

    #[test]
    fn user_edit_allows_partial_known_projects_but_rejects_unstructured_text() {
        let known = vec!["App".into(), "Docs".into()];
        let ok = "# Daily Report · 2026-11-16\n\n## App\n- Shipped v1 [#S1.T0]\n";
        let validated = validate_user_edit(ok, &daily_title("2026-11-16"), &known).unwrap();
        assert_eq!(validated.sections.len(), 1);
        assert_eq!(validated.sections[0].project, "App");

        let bad = "# Daily Report\n\n## UnknownProj\n- did work\n";
        assert!(validate_user_edit(bad, &daily_title("2026-11-16"), &known).is_err());
    }

    #[test]
    fn extracts_project_section_and_builds_rollup() {
        let day1 = "# Daily · 2026-11-16\n\n## Lantai\n- Built rollup view [#S4.T1]\n\n## Atlas\n- Fixed eval [#S9.T0]";
        let day2 = "# Daily · 2026-11-15\n\n## Atlas\n- Cleaned bib [#S2.T0]";

        assert_eq!(
            extract_project_section(day1, "Lantai").as_deref(),
            Some("- Built rollup view [#S4.T1]")
        );
        assert_eq!(extract_project_section(day2, "Lantai"), None);

        let entries = vec![
            (
                "2026-11-16".into(),
                extract_project_section(day1, "Atlas").unwrap(),
            ),
            (
                "2026-11-15".into(),
                extract_project_section(day2, "Atlas").unwrap(),
            ),
        ];
        let rollup = build_project_rollup("Atlas", "last-30d", &entries);
        assert_eq!(
            rollup,
            "# Atlas · last-30d\n\n## 2026-11-16\n- Fixed eval [#S9.T0]\n\n## 2026-11-15\n- Cleaned bib [#S2.T0]"
        );
    }
}
