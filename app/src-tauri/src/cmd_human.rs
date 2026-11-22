//! Thin Tauri commands over `aj_report::human`: the notes and annotations people write next
//! to the generated reports. Every command works in a scope, one period (a report view) or one
//! project page (`aj_report::human::project`), and every change returns the scope's nodes
//! again, so the interface never has to guess the new order.

use aj_report::human::project::{self as project_journal, ProjectPlace};
use aj_report::human::{self, attach::Placement, Drop, HumanView, Move, NewPlace};
use aj_report::rollup::Granularity;
use aj_store::{HumanNodeRow, HumanPosition, Store};
use serde::{Deserialize, Serialize};

use crate::state::{cmd_err, AppState};

/// Where a project note shows in its project's section (mirrors [`HumanPosition`]).
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum PositionParam {
    Top,
    Bottom,
}

impl From<HumanPosition> for PositionParam {
    fn from(p: HumanPosition) -> Self {
        match p {
            HumanPosition::Top => PositionParam::Top,
            HumanPosition::Bottom => PositionParam::Bottom,
        }
    }
}

impl From<PositionParam> for HumanPosition {
    fn from(p: PositionParam) -> Self {
        match p {
            PositionParam::Top => HumanPosition::Top,
            PositionParam::Bottom => HumanPosition::Bottom,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanNodeDto {
    pub id: i64,
    /// `day` | `week` | `month`, or `project` for the notes of a project's own page.
    pub period_kind: String,
    pub period_key: String,
    pub project_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub sort_key: String,
    pub body: String,
    /// A root written on a point of the generated report.
    pub is_annotation: bool,
    /// Top or bottom of the project's section (children copy their root's).
    pub position: PositionParam,
    /// The point's text when the annotation was written (annotations only).
    pub quote: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Where an annotation shows: under the point on `line`, at the end of the section whose
/// heading is on `line`, or apart from the report (`line` absent).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementDto {
    pub id: i64,
    /// `point` | `section` | `detached`
    pub kind: &'static str,
    pub line: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanViewDto {
    pub nodes: Vec<HumanNodeDto>,
    pub placements: Vec<PlacementDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanCreatedDto {
    pub id: i64,
    pub view: HumanViewDto,
}

/// The nodes a command works on: one period, or one project page (its own notes and the
/// project's nodes of every `granularity` period).
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ScopeParam {
    Period {
        kind: String,
        period: String,
    },
    Project {
        project_id: i64,
        granularity: String,
    },
}

/// Where a new node goes (mirrors [`NewPlace`] and [`ProjectPlace`]). In a project scope,
/// `period` says which period a note or annotation is for; without it a project note goes on
/// the project's page.
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NewPlaceParam {
    DayNote,
    ProjectNote {
        project_id: i64,
        period: Option<String>,
        /// Top or bottom of the section (the bottom when absent, and always on a project's
        /// own page).
        position: Option<PositionParam>,
    },
    Annotation {
        line: usize,
        period: Option<String>,
    },
    After {
        id: i64,
    },
    LastChild {
        id: i64,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveParam {
    Up,
    Down,
    Indent,
    Outdent,
}

fn view_dto(view: HumanView) -> HumanViewDto {
    let placements = view
        .placements
        .iter()
        .map(|(&id, p)| {
            let (kind, line) = match *p {
                Placement::Point(l) => ("point", Some(l)),
                Placement::Section(l) => ("section", Some(l)),
                Placement::Detached => ("detached", None),
            };
            PlacementDto { id, kind, line }
        })
        .collect();
    let nodes = view
        .nodes
        .into_iter()
        .map(|n| {
            let quote = n
                .attach_json
                .as_deref()
                .and_then(human::attach::Attachment::parse)
                .map(|a| a.quote);
            HumanNodeDto {
                id: n.id,
                period_kind: n.period_kind,
                period_key: n.period_key,
                project_id: n.project_id,
                parent_id: n.parent_id,
                sort_key: n.sort_key,
                body: n.body,
                is_annotation: n.parent_id.is_none() && n.attach_json.is_some(),
                position: n.position.into(),
                quote,
                created_at: n.created_at,
                updated_at: n.updated_at,
            }
        })
        .collect();
    HumanViewDto { nodes, placements }
}

fn position_or_bottom(p: Option<PositionParam>) -> HumanPosition {
    p.map(HumanPosition::from).unwrap_or_default()
}

fn load(store: &Store, scope: &ScopeParam) -> Result<HumanViewDto, String> {
    let view = match scope {
        ScopeParam::Period { kind, period } => human::load(store, kind, period),
        ScopeParam::Project {
            project_id,
            granularity,
        } => Granularity::parse(granularity)
            .and_then(|g| project_journal::load(store, *project_id, g)),
    };
    view.map(view_dto).map_err(cmd_err)
}

/// The nodes of a scope with where each annotation shows.
#[tauri::command(async)]
pub fn human_load(
    state: tauri::State<'_, AppState>,
    scope: ScopeParam,
) -> Result<HumanViewDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    load(&store, &scope)
}

#[tauri::command(async)]
pub fn human_create(
    state: tauri::State<'_, AppState>,
    scope: ScopeParam,
    place: NewPlaceParam,
    body: String,
) -> Result<HumanCreatedDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let id = match &scope {
        ScopeParam::Period { kind, period } => {
            let place = match place {
                NewPlaceParam::DayNote => NewPlace::DayNote,
                NewPlaceParam::ProjectNote {
                    project_id,
                    period: None,
                    position,
                } => NewPlace::ProjectNote(project_id, position_or_bottom(position)),
                NewPlaceParam::Annotation { line, period: None } => NewPlace::Annotation(line),
                NewPlaceParam::After { id } => NewPlace::After(id),
                NewPlaceParam::LastChild { id } => NewPlace::LastChild(id),
                _ => return Err("a period view places nodes in its own period".into()),
            };
            human::create(&store, kind, period, place, &body)
        }
        ScopeParam::Project {
            project_id,
            granularity,
        } => {
            let place = match place {
                NewPlaceParam::ProjectNote {
                    project_id: p,
                    period,
                    position,
                } if p == *project_id => match period {
                    Some(period) => ProjectPlace::PeriodNote(period, position_or_bottom(position)),
                    None => ProjectPlace::PageNote,
                },
                NewPlaceParam::Annotation {
                    line,
                    period: Some(period),
                } => ProjectPlace::Annotation { period, line },
                NewPlaceParam::After { id } => ProjectPlace::After(id),
                NewPlaceParam::LastChild { id } => ProjectPlace::LastChild(id),
                _ => return Err("a project page only holds its own project's nodes".into()),
            };
            Granularity::parse(granularity)
                .and_then(|g| project_journal::create(&store, *project_id, g, place, &body))
        }
    }
    .map_err(cmd_err)?;
    Ok(HumanCreatedDto {
        id,
        view: load(&store, &scope)?,
    })
}

#[tauri::command(async)]
pub fn human_update(
    state: tauri::State<'_, AppState>,
    scope: ScopeParam,
    id: i64,
    body: String,
) -> Result<HumanViewDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    match &scope {
        ScopeParam::Period { kind, period } => human::update_body(&store, kind, period, id, &body),
        ScopeParam::Project { .. } => project_journal::update_body(&store, id, &body),
    }
    .map_err(cmd_err)?;
    load(&store, &scope)
}

#[tauri::command(async)]
pub fn human_move(
    state: tauri::State<'_, AppState>,
    scope: ScopeParam,
    id: i64,
    direction: MoveParam,
) -> Result<HumanViewDto, String> {
    let m = match direction {
        MoveParam::Up => Move::Up,
        MoveParam::Down => Move::Down,
        MoveParam::Indent => Move::Indent,
        MoveParam::Outdent => Move::Outdent,
    };
    let mut store = state.open_store().map_err(cmd_err)?;
    match &scope {
        ScopeParam::Period { kind, period } => human::move_node(&mut store, kind, period, id, m),
        ScopeParam::Project { .. } => project_journal::move_node(&mut store, id, m),
    }
    .map_err(cmd_err)?;
    load(&store, &scope)
}

/// Where dragged nodes are dropped (mirrors [`Drop`]).
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DropParam {
    Notes {
        project_id: Option<i64>,
        position: PositionParam,
        after: Option<i64>,
    },
    Annotations {
        with: i64,
        after: Option<i64>,
    },
    Children {
        parent: i64,
        after: Option<i64>,
    },
}

/// Move dragged nodes, each with everything under it, to one place.
#[tauri::command(async)]
pub fn human_move_to(
    state: tauri::State<'_, AppState>,
    scope: ScopeParam,
    ids: Vec<i64>,
    drop: DropParam,
) -> Result<HumanViewDto, String> {
    let drop = match drop {
        DropParam::Notes {
            project_id,
            position,
            after,
        } => Drop::Notes {
            project_id,
            position: position.into(),
            after,
        },
        DropParam::Annotations { with, after } => Drop::Annotations { with, after },
        DropParam::Children { parent, after } => Drop::Children { parent, after },
    };
    let mut store = state.open_store().map_err(cmd_err)?;
    match &scope {
        ScopeParam::Period { kind, period } => human::move_to(&mut store, kind, period, &ids, drop),
        ScopeParam::Project { .. } => project_journal::move_to(&mut store, &ids, drop),
    }
    .map_err(cmd_err)?;
    load(&store, &scope)
}

/// A removed node exactly as stored, kept by the interface until it is restored (undo).
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedNodeDto {
    pub id: i64,
    pub period_kind: String,
    pub period_key: String,
    pub project_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub sort_key: String,
    pub attach_json: Option<String>,
    pub position: PositionParam,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<HumanNodeRow> for RemovedNodeDto {
    fn from(n: HumanNodeRow) -> Self {
        Self {
            id: n.id,
            period_kind: n.period_kind,
            period_key: n.period_key,
            project_id: n.project_id,
            parent_id: n.parent_id,
            sort_key: n.sort_key,
            attach_json: n.attach_json,
            position: n.position.into(),
            body: n.body,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}

impl From<RemovedNodeDto> for HumanNodeRow {
    fn from(n: RemovedNodeDto) -> Self {
        Self {
            id: n.id,
            period_kind: n.period_kind,
            period_key: n.period_key,
            project_id: n.project_id,
            parent_id: n.parent_id,
            sort_key: n.sort_key,
            attach_json: n.attach_json,
            position: n.position.into(),
            body: n.body,
            created_at: n.created_at,
            updated_at: n.updated_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HumanDeletedDto {
    pub removed: Vec<RemovedNodeDto>,
    pub view: HumanViewDto,
}

/// Delete nodes with everything under them; the removed rows come back for an undo.
#[tauri::command(async)]
pub fn human_delete(
    state: tauri::State<'_, AppState>,
    scope: ScopeParam,
    ids: Vec<i64>,
) -> Result<HumanDeletedDto, String> {
    let mut store = state.open_store().map_err(cmd_err)?;
    let removed = human::delete(&mut store, &ids).map_err(cmd_err)?;
    Ok(HumanDeletedDto {
        removed: removed.into_iter().map(RemovedNodeDto::from).collect(),
        view: load(&store, &scope)?,
    })
}

/// Put back nodes an earlier [`human_delete`] removed.
#[tauri::command(async)]
pub fn human_restore(
    state: tauri::State<'_, AppState>,
    scope: ScopeParam,
    rows: Vec<RemovedNodeDto>,
) -> Result<HumanViewDto, String> {
    let rows: Vec<HumanNodeRow> = rows.into_iter().map(HumanNodeRow::from).collect();
    let mut store = state.open_store().map_err(cmd_err)?;
    human::restore(&mut store, &rows).map_err(cmd_err)?;
    load(&store, &scope)
}
