//! Report versions as people see and change them: each stored version in its display form,
//! with the lines the user wrote and the models that wrote its sections; and saving a user's
//! edit as a new version.
//!
//! Only period reports (daily, weekly, monthly) have a display form and edit marks; other
//! reports are shown and saved as stored.

use aj_store::{NewReportVersion, ReportOrigin, ReportVersionRow, Store};
use anyhow::{anyhow, Result};

use crate::edits::{report_edits, ReportEdits};
use crate::lang::Language;
use crate::manifest::{section_writers, Manifest};
use crate::prompt_library::{KIND_DAILY, KIND_MONTHLY, KIND_WEEKLY};
use crate::report_io::{display_markdown, project_name, project_order, version_language};
use crate::structure::{report_title, validate_user_edit};

/// A stored version as it is shown.
#[derive(Debug, Clone)]
pub struct ShownVersion {
    pub version: ReportVersionRow,
    /// The Markdown as shown: for period reports the canonical title in the version's
    /// language and project sections in the user's order (see [`display_markdown`]).
    pub markdown: String,
    /// Lines the user wrote rather than the model; only on the current version of a period
    /// report, and only when its manifest allows telling them apart.
    pub edits: Option<ReportEdits>,
    /// Who wrote the sections: one entry, or several after an Update with another model.
    pub writers: Vec<ShownWriter>,
}

/// A model that wrote sections of a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShownWriter {
    /// `None` for sections kept from a version that did not record its model.
    pub engine: Option<String>,
    /// Current names of the projects whose sections it wrote; empty when not recorded.
    pub projects: Vec<String>,
}

/// Whether reports of `kind` have a display form and edit marks.
fn is_period_report(kind: &str) -> bool {
    matches!(kind, KIND_DAILY | KIND_WEEKLY | KIND_MONTHLY)
}

/// Every version of a report, oldest first, as it is shown.
pub fn shown_versions(
    store: &Store,
    kind: &str,
    period: &str,
    project_id: Option<i64>,
) -> Result<Vec<ShownVersion>> {
    let period_report = is_period_report(kind);
    let order = if period_report {
        project_order(store)?
    } else {
        Vec::new()
    };
    let rows = store.report_versions(kind, period, project_id)?;
    // Newest first, for finding the generated text of each section.
    let history: Vec<&str> = rows.iter().rev().map(|v| v.markdown.as_str()).collect();
    let mut shown = Vec::with_capacity(rows.len());
    for v in &rows {
        let markdown = if period_report {
            display_markdown(kind, period, &v.markdown, &v.language, &order)
        } else {
            v.markdown.clone()
        };
        let edits = if period_report && v.is_current {
            report_edits(&markdown, v.input_manifest.as_deref(), &history).filter(|e| !e.is_empty())
        } else {
            None
        };
        let writers = writers(store, v)?;
        shown.push((markdown, edits, writers));
    }
    Ok(rows
        .into_iter()
        .zip(shown)
        .map(|(version, (markdown, edits, writers))| ShownVersion {
            version,
            markdown,
            edits,
            writers,
        })
        .collect())
}

/// A report's text as it is shown (see [`display_markdown`]), for printing or writing a report
/// outside the app. Reports other than period reports come back as they are.
pub fn display_text(
    store: &Store,
    kind: &str,
    period: &str,
    markdown: &str,
    language: &str,
) -> Result<String> {
    if !is_period_report(kind) {
        return Ok(markdown.to_string());
    }
    Ok(display_markdown(
        kind,
        period,
        markdown,
        language,
        &project_order(store)?,
    ))
}

/// The models that wrote a version's sections, with the projects named as they are now.
fn writers(store: &Store, version: &ReportVersionRow) -> Result<Vec<ShownWriter>> {
    let manifest = version.input_manifest.as_deref().and_then(Manifest::parse);
    section_writers(manifest.as_ref(), version.engine.as_deref())
        .into_iter()
        .map(|w| {
            let projects = match &manifest {
                Some(m) => w
                    .keys
                    .iter()
                    .filter_map(|k| m.projects.get(k).map(|e| (k, e)))
                    .map(|(k, e)| project_name(store, k, e))
                    .collect::<Result<Vec<_>>>()?,
                None => Vec::new(),
            };
            Ok(ShownWriter {
                engine: w.engine,
                projects,
            })
        })
        .collect()
}

/// Save `markdown` as the user's edit of a report and return the new version's ID.
///
/// The edit is based on the current version: it keeps that version's language, prompt name
/// and input manifest, so a later Update still knows what the kept sections came from.
/// `language` is used only when the report has no version yet. A period report must keep
/// valid project headings and bullet items; its title is rewritten to the canonical one.
pub fn save_user_edit(
    store: &mut Store,
    kind: &str,
    period: &str,
    project_id: Option<i64>,
    markdown: &str,
    language: &'static Language,
) -> Result<i64> {
    let current = store.current_report_version(kind, period, project_id)?;
    let language = match &current {
        Some(v) => version_language(&v.language),
        None => language,
    };
    let markdown = if is_period_report(kind) {
        validate_user_edit(
            markdown,
            &report_title(kind, period, language),
            &project_order(store)?,
        )
        .map_err(|errs| {
            anyhow!(
                "Report edit must keep valid project headings (## <Project>) and \"- \" bullet items: {}",
                errs.join("; ")
            )
        })?
        .markdown
    } else {
        markdown.to_string()
    };
    let prompt_name = current.as_ref().and_then(|v| v.prompt_name.clone());
    let manifest = current.and_then(|v| v.input_manifest);
    store.save_report_version(&NewReportVersion {
        report_type: kind,
        period_key: period,
        project_id,
        origin: ReportOrigin::UserEdit,
        markdown: &markdown,
        engine: None,
        input_hash: None,
        prompt_id: None,
        prompt_name: prompt_name.as_deref(),
        prompt_snapshot: None,
        input_manifest: manifest.as_deref(),
        language: language.tag,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::EN;

    fn generated(store: &mut Store, markdown: &str) {
        store
            .save_report_version(&NewReportVersion {
                report_type: KIND_DAILY,
                period_key: "2026-11-16",
                project_id: None,
                origin: ReportOrigin::Generated,
                markdown,
                engine: Some("model-a"),
                input_hash: None,
                prompt_id: None,
                prompt_name: Some("standard"),
                prompt_snapshot: None,
                input_manifest: None,
                language: "en",
            })
            .unwrap();
    }

    #[test]
    fn period_versions_are_shown_with_the_canonical_title_and_project_order() {
        let mut store = Store::open_in_memory().unwrap();
        store.create_project("Beta", None).unwrap();
        store.create_project("Alpha", None).unwrap();
        generated(
            &mut store,
            "# Model title\n\n## Alpha\n\n- a point\n\n## Beta\n\n- b point\n",
        );
        let shown = shown_versions(&store, KIND_DAILY, "2026-11-16", None).unwrap();
        assert_eq!(shown.len(), 1);
        let md = &shown[0].markdown;
        assert!(!md.contains("Model title"));
        assert!(md.find("## Beta").unwrap() < md.find("## Alpha").unwrap());
        assert_eq!(
            shown[0].writers,
            [ShownWriter {
                engine: Some("model-a".into()),
                projects: vec![],
            }]
        );
    }

    #[test]
    fn display_text_matches_the_shown_version() {
        let mut store = Store::open_in_memory().unwrap();
        store.create_project("Beta", None).unwrap();
        store.create_project("Alpha", None).unwrap();
        let stored = "# Model title\n\n## Alpha\n\n- a point\n\n## Beta\n\n- b point\n";
        generated(&mut store, stored);
        let shown = shown_versions(&store, KIND_DAILY, "2026-11-16", None).unwrap();
        let text = display_text(&store, KIND_DAILY, "2026-11-16", stored, "en").unwrap();
        assert_eq!(text, shown[0].markdown);
        // Reports without a display form come back unchanged.
        assert_eq!(
            display_text(&store, "other", "2026-11-16", stored, "en").unwrap(),
            stored
        );
    }

    #[test]
    fn a_user_edit_keeps_the_current_versions_prompt_and_is_validated() {
        let mut store = Store::open_in_memory().unwrap();
        store.create_project("Alpha", None).unwrap();
        generated(&mut store, "# Title\n\n## Alpha\n\n- a point\n");

        let id = save_user_edit(
            &mut store,
            KIND_DAILY,
            "2026-11-16",
            None,
            "# Anything\n\n## Alpha\n\n- a better point\n",
            &EN,
        )
        .unwrap();
        let saved = store
            .current_report_version(KIND_DAILY, "2026-11-16", None)
            .unwrap()
            .unwrap();
        assert_eq!(saved.id, id);
        assert_eq!(saved.origin, ReportOrigin::UserEdit);
        assert_eq!(saved.prompt_name.as_deref(), Some("standard"));
        assert!(saved.markdown.contains("a better point"));
        assert!(!saved.markdown.contains("Anything"));

        let err = save_user_edit(
            &mut store,
            KIND_DAILY,
            "2026-11-16",
            None,
            "## Nobody knows this project\n\n- a point\n",
            &EN,
        )
        .unwrap_err();
        assert!(err.to_string().contains("project headings"));
    }
}
