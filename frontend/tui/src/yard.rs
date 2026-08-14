//! In-process shipyard: browse existing designs or create/edit one.
//! Cost and space come from `shipsim_core::shipyard`, not from combat.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use shipsim_core::shipyard::{
    self, Design, DesignPreview, DesignSystem, DesignWeapon, EngineSpec, SystemSpec, WeaponSpec,
    ENGINE_KINDS, ENGINE_SIZES, MATERIALS, MOUNTS,
};
use shipsim_core::sizes::{self, SizeTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YardScreen {
    Browse,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Recency,
    Size,
    Cost,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Recency => "recency",
            Self::Size => "size",
            Self::Cost => "cost",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Recency => Self::Size,
            Self::Size => Self::Cost,
            Self::Cost => Self::Recency,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Name,
    Size,
    Material,
    Engine,
    Armor,
    Shields,
    Weapon { index: usize },
    System { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YardRowKind {
    Field(EditField),
    Section,
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YardRow {
    pub kind: YardRowKind,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub weapons: Vec<WeaponSpec>,
    pub systems: Vec<SystemSpec>,
    pub engines: Vec<EngineSpec>,
    pub weapon_headlines: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Weapon { index: usize },
    Hull,
    Material,
    Engine,
    System { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerRow {
    pub id: String,
    pub cells: Vec<String>,
    pub fits: bool,
}

#[derive(Debug, Clone)]
pub struct Picker {
    pub kind: PickerKind,
    pub title: String,
    pub headers: Vec<&'static str>,
    pub rows: Vec<PickerRow>,
    pub cursor: usize,
    pub scroll: usize,
    pub filter: String,
    pub filtering: bool,
    pub original: Design,
    pub original_id: String,
}

pub fn column_widths(headers: &[&str], rows: &[PickerRow], max_total: usize) -> Vec<usize> {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.chars().count()).collect();
    for row in rows {
        for (index, cell) in row.cells.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(cell.chars().count());
            }
        }
    }
    let available = max_total.saturating_sub(widths.len().saturating_sub(1) * 2);
    while widths.iter().sum::<usize>() > available {
        let Some((index, _)) = widths.iter().enumerate().max_by_key(|(_, width)| **width) else { break };
        if widths[index] <= headers[index].chars().count().max(3) { break; }
        widths[index] -= 1;
    }
    widths
}

impl Catalog {
    pub fn load(root: &Path) -> Result<Self, String> {
        let weapon_ids = shipyard::weapon_skus(root).map_err(|e| e.to_string())?;
        let system_ids = shipyard::system_skus(root).map_err(|e| e.to_string())?;
        let weapons: Vec<_> = weapon_ids.iter().filter_map(|id| shipyard::weapon_spec(root, id).ok()).collect();
        let systems: Vec<_> = system_ids.iter().filter_map(|id| shipyard::system_spec(root, id).ok()).collect();
        let mut engines = Vec::new();
        for kind in ENGINE_KINDS {
            for size in ENGINE_SIZES {
                engines.push(shipyard::engine_spec(root, kind, size).map_err(|e| e.to_string())?);
            }
        }
        let weapon_headlines = weapons.iter().filter_map(|spec| {
            shipyard::weapon_headline_from_spec(root, spec).ok().map(|headline| (spec.id.clone(), headline))
        }).collect();
        Ok(Self { weapons, systems, engines, weapon_headlines })
    }

    pub fn weapon(&self, id: &str) -> Option<&WeaponSpec> { self.weapons.iter().find(|spec| spec.id == id) }
    pub fn system(&self, id: &str) -> Option<&SystemSpec> { self.systems.iter().find(|spec| spec.id == id) }
    pub fn engine(&self, kind: &str, size: &str) -> Option<&EngineSpec> { self.engines.iter().find(|spec| spec.kind == kind && spec.size == size) }
    pub fn weapon_headline(&self, id: &str) -> &str { self.weapon_headlines.get(id).map(String::as_str).unwrap_or("") }
}

pub fn clamp_scroll(current: usize, cursor: usize, len: usize, height: usize) -> usize {
    if height == 0 || len == 0 {
        return 0;
    }
    let max_offset = len.saturating_sub(height);
    let mut offset = current.min(max_offset);
    if cursor < offset {
        offset = cursor;
    } else if cursor >= offset + height {
        offset = cursor + 1 - height;
    }
    offset.min(max_offset)
}

/// An action armed by one keypress, awaiting a second confirming keypress.
/// Any other key cancels it (see `input.rs`) rather than leaving it armed
/// indefinitely, so a later unrelated `d` can never land on a stale arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingConfirm {
    DiscardChanges,
    DeleteWeapon { index: usize },
    DeleteSystem { index: usize },
    DeleteDesign { index: usize },
}

#[derive(Debug, Clone)]
pub struct ListedDesign {
    pub path: PathBuf,
    pub design: Design,
    pub preview: Result<DesignPreview, String>,
}

#[derive(Debug, Clone)]
pub struct YardState {
    pub root: PathBuf,
    pub screen: YardScreen,
    pub listings: Vec<ListedDesign>,
    /// Browse cursor; last row is "new ship" if `listings.len()` is selected.
    pub browse_cursor: usize,
    pub draft: Design,
    /// The draft as last loaded, saved, or compiled — the baseline `is_dirty`
    /// compares against so Esc only needs to warn when there is something to
    /// lose.
    pub saved: Design,
    pub edit_cursor: EditField,
    pub status: String,
    pub skus: Vec<String>,
    pub system_skus: Vec<String>,
    pub sizes: SizeTable,
    pub pending: Option<PendingConfirm>,
    pub viewing_readonly: bool,
    pub sort_mode: SortMode,
    pub edit_scroll: usize,
    pub catalog: Catalog,
    pub picker: Option<Picker>,
    pub shield_editor: Option<usize>,
}

impl YardState {
    pub fn load(root: PathBuf) -> Result<Self, String> {
        let skus = shipyard::weapon_skus(&root).map_err(|e| e.to_string())?;
        let system_skus = shipyard::system_skus(&root).map_err(|e| e.to_string())?;
        let sizes = sizes::load(&root).map_err(|e| e.to_string())?;
        let catalog = Catalog::load(&root)?;
        let draft = shipyard::new_design("yard_custom");
        let mut yard = Self {
            root,
            screen: YardScreen::Browse,
            listings: Vec::new(),
            browse_cursor: 0,
            saved: draft.clone(),
            draft,
            edit_cursor: EditField::Size,
            status: String::new(),
            skus,
            system_skus,
            sizes,
            pending: None,
            viewing_readonly: false,
            sort_mode: SortMode::Recency,
            edit_scroll: 0,
            catalog,
            picker: None,
            shield_editor: None,
        };
        yard.refresh_listings();
        Ok(yard)
    }

    pub fn refresh_listings(&mut self) {
        match shipyard::list_designs(&self.root) {
            Ok(list) => {
                let selected_id = self
                    .listings
                    .get(self.browse_cursor)
                    .map(|item| item.design.id.clone());
                let mut designs = list
                    .into_iter()
                    // Weapon-quality fixtures (yard_baseline/compact/potent/precise)
                    // are balance-suite controls, not player-facing standards —
                    // keep them off the interactive picker so "one of each
                    // standard type" is what a player actually sees. They are
                    // untouched on disk and still visible to the CLI.
                    .filter(|(_, design)| {
                        !shipyard::QUALITY_FIXTURE_IDS.contains(&design.id.as_str())
                    })
                    .map(|(path, design)| {
                        let preview = shipyard::preview_design(&self.root, &design)
                            .map_err(|e| e.to_string());
                        ListedDesign {
                            path,
                            design,
                            preview,
                        }
                    })
                    .collect::<Vec<_>>();
                designs.sort_by(|a, b| {
                    let a_standard = shipyard::STANDARD_CLASS_IDS.contains(&a.design.id.as_str());
                    let b_standard = shipyard::STANDARD_CLASS_IDS.contains(&b.design.id.as_str());
                    match (a_standard, b_standard) {
                        (true, false) => std::cmp::Ordering::Less,
                        (false, true) => std::cmp::Ordering::Greater,
                        (true, true) => a.design.size.cmp(&b.design.size).then(a.design.id.cmp(&b.design.id)),
                        (false, false) => self.compare_user_designs(a, b),
                    }
                });
                self.listings = designs;
                if let Some(id) = selected_id {
                    if let Some(index) = self.listings.iter().position(|item| item.design.id == id) {
                        self.browse_cursor = index;
                    }
                }
                if self.browse_cursor > self.listings.len() {
                    self.browse_cursor = self.listings.len();
                }
                self.status = format!("{} saved design(s) · sort: {} — o change · Enter edit · n new", self.listings.len(), self.sort_mode.label());
            }
            Err(err) => {
                self.listings.clear();
                self.status = err.to_string();
            }
        }
    }

    fn compare_user_designs(&self, a: &ListedDesign, b: &ListedDesign) -> std::cmp::Ordering {
        let ordering = match self.sort_mode {
            SortMode::Recency => metadata_time(&b.path).cmp(&metadata_time(&a.path)),
            SortMode::Size => a.design.size.cmp(&b.design.size),
            SortMode::Cost => preview_cost(&a.preview).cmp(&preview_cost(&b.preview)),
        };
        ordering.then(a.design.id.cmp(&b.design.id))
    }

    pub fn cycle_sort(&mut self) {
        if self.screen != YardScreen::Browse {
            return;
        }
        self.sort_mode = self.sort_mode.next();
        self.refresh_listings();
    }

    pub fn is_readonly(&self) -> bool {
        self.viewing_readonly
    }

    pub fn open_picker(&mut self) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        if self.edit_cursor == EditField::Shields {
            self.open_shield_editor();
            return;
        }
        let original = self.draft.clone();
        let (kind, title, headers, original_id) = match self.edit_cursor {
            EditField::Weapon { index } => (PickerKind::Weapon { index }, format!("choose weapon, slot {}", index + 1), vec!["sku", "kind", "damage", "space", "cost", "ammo", "tags"], self.draft.weapons.get(index).map(|weapon| weapon.component.clone()).unwrap_or_default()),
            EditField::Size => (PickerKind::Hull, "choose hull".into(), vec!["hull", "space", "hp", "move", "def", "frame cost"], self.draft.size.to_string()),
            EditField::Material => (PickerKind::Material, "choose material".into(), vec!["material", "tech", "hp mult", "cost mult"], self.draft.material.clone()),
            EditField::Engine => (PickerKind::Engine, "choose engine".into(), vec!["plant", "power", "space", "cost", "thrust"], format!("{}_{}", self.draft.engine, self.draft.engine_size)),
            EditField::System { index } => (PickerKind::System { index }, format!("choose system, slot {}", index + 1), vec!["system", "effect", "space", "cost"], self.draft.systems.get(index).map(|system| system.component.clone()).unwrap_or_default()),
            _ => return,
        };
        self.picker = Some(Picker { kind, title, headers, rows: Vec::new(), cursor: 0, scroll: 0, filter: String::new(), filtering: false, original, original_id });
        self.rebuild_picker_rows();
        if let Some(picker) = self.picker.as_mut() {
            picker.cursor = picker.rows.iter().position(|row| row.id == picker.original_id).unwrap_or(0);
        }
    }

    pub fn open_shield_editor(&mut self) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        self.shield_editor = Some(0);
    }

    pub fn shield_face_move(&mut self, delta: i32) {
        if self.viewing_readonly { self.status = "standard classes are reference-only".into(); return; }
        if let Some(face) = self.shield_editor.as_mut() { *face = (*face as i32 + delta).rem_euclid(6) as usize; }
    }

    pub fn shield_face_adjust(&mut self, delta: i32) {
        if self.viewing_readonly { self.status = "standard classes are reference-only".into(); return; }
        if let Some(face) = self.shield_editor { let value = self.draft.shields[face] as i64 + i64::from(delta); self.set_shield_face(face, value.clamp(0, 40) as u64); }
    }

    pub fn shield_set_all(&mut self, value: u64) {
        if self.viewing_readonly { self.status = "standard classes are reference-only".into(); return; }
        for face in &mut self.draft.shields { *face = value.min(40); }
    }

    pub fn close_shield_editor(&mut self) { self.shield_editor = None; }

    fn rebuild_picker_rows(&mut self) {
        let Some(mut picker) = self.picker.take() else { return };
        let filter = picker.filter.to_ascii_lowercase();
        let weapon_index = match picker.kind { PickerKind::Weapon { index } => Some(index), _ => None };
        let old_space = weapon_index.and_then(|index| self.draft.weapons.get(index)).and_then(|weapon| self.catalog.weapon(&weapon.component)).map(|spec| spec.space).unwrap_or(0);
        let used = self.preview().ok().map(|preview| preview.space_used).unwrap_or(u32::MAX);
        picker.rows = match picker.kind {
            PickerKind::Weapon { .. } => self.catalog.weapons.iter().filter(|spec| filter.is_empty() || spec.id.to_ascii_lowercase().contains(&filter) || spec.kind.to_ascii_lowercase().contains(&filter)).map(|spec| PickerRow { id: spec.id.clone(), cells: vec![spec.id.clone(), spec.kind.clone(), self.catalog.weapon_headline(&spec.id).to_string(), spec.space.to_string(), spec.cost.to_string(), spec.max_ammo.map(|ammo| ammo.to_string()).unwrap_or_else(|| "-".into()), spec.quality_tags().join(" ")], fits: used.saturating_sub(old_space).saturating_add(spec.space) <= self.sizes.get(self.draft.size).map(|h| h.space).unwrap_or(0) }).collect(),
            PickerKind::Hull => self.sizes.sizes.iter().map(|hull| PickerRow { id: hull.id.to_string(), cells: vec![hull.name.clone(), hull.space.to_string(), hull.base_structure.to_string(), hull.max_maneuver_actions.to_string(), hull.defense.to_string(), hull.frame_cost.to_string()], fits: true }).collect(),
            PickerKind::Material => MATERIALS.iter().map(|mat| PickerRow { id: mat.id.into(), cells: vec![mat.name.into(), mat.tech.to_string(), format!("{:.1}x", mat.structure_mult), format!("{:.1}x", mat.cost_mult)], fits: true }).collect(),
            PickerKind::Engine => self.catalog.engines.iter().map(|spec| PickerRow { id: spec.id.clone(), cells: vec![spec.id.clone(), spec.power.to_string(), spec.space.to_string(), spec.cost.to_string(), spec.thrust_step.to_string()], fits: true }).collect(),
            PickerKind::System { index } => self.draft.systems.get(index).map(|system| self.available_system_skus(Some(&system.component))).unwrap_or_default().iter().filter_map(|id| self.catalog.system(id)).map(|spec| PickerRow { id: spec.id.clone(), cells: vec![spec.id.clone(), spec.headline(), spec.space.to_string(), spec.cost.to_string()], fits: true }).collect(),
        };
        if picker.rows.is_empty() { picker.cursor = 0; } else { picker.cursor = picker.cursor.min(picker.rows.len() - 1); }
        self.picker = Some(picker);
    }

    pub fn picker_move(&mut self, delta: i32) {
        let Some(picker) = self.picker.as_mut() else { return };
        if picker.rows.is_empty() { return; }
        picker.cursor = (picker.cursor as i32 + delta).clamp(0, picker.rows.len() as i32 - 1) as usize;
        let id = picker.rows[picker.cursor].id.clone();
        match picker.kind {
            PickerKind::Weapon { index } => if let Some(weapon) = self.draft.weapons.get_mut(index) { weapon.component = id; },
            PickerKind::Hull => { let old = self.draft.size; self.draft.size = id.parse().unwrap_or(old); self.retitle_if_generated(old, self.draft.size); },
            PickerKind::Material => self.draft.material = id,
            PickerKind::Engine => if let Some(spec) = self.catalog.engines.iter().find(|spec| spec.id == id) { self.draft.engine = spec.kind.clone(); self.draft.engine_size = spec.size.clone(); },
            PickerKind::System { index } => if let Some(system) = self.draft.systems.get_mut(index) { system.component = id; },
        }
    }

    pub fn picker_commit(&mut self) { self.picker = None; }

    pub fn picker_cancel(&mut self) {
        if let Some(picker) = self.picker.take() { self.draft = picker.original; }
    }

    pub fn picker_type(&mut self, ch: char) {
        if let Some(picker) = self.picker.as_mut() {
            picker.filter.push(ch);
            picker.cursor = 0;
        }
        self.rebuild_picker_rows();
    }

    pub fn picker_backspace(&mut self) {
        if let Some(picker) = self.picker.as_mut() {
            picker.filter.pop();
            picker.cursor = 0;
        }
        self.rebuild_picker_rows();
    }

    pub fn picker_clear_filter(&mut self) {
        if let Some(picker) = self.picker.as_mut() {
            picker.filter.clear();
            picker.filtering = false;
            picker.cursor = 0;
        }
        self.rebuild_picker_rows();
    }

    pub fn picker_delta_line(&self) -> String {
        let Some(picker) = self.picker.as_ref() else { return String::new() };
        let PickerKind::Weapon { index } = picker.kind else { return String::new() };
        let Some(current) = picker.original.weapons.get(index) else { return String::new() };
        let candidate = self.draft.weapons.get(index).map(|weapon| weapon.component.as_str()).unwrap_or("");
        let old = self.catalog.weapon(&current.component).map(|spec| format!("{} ({}sp {}c)", current.component, spec.space, spec.cost)).unwrap_or_else(|| current.component.clone());
        let new = self.catalog.weapon(candidate).map(|spec| format!("{} ({}sp {}c)", candidate, spec.space, spec.cost)).unwrap_or_else(|| candidate.into());
        let preview = self.preview().ok();
        format!("{old} -> {new}   space {}/{}   cost {}", preview.as_ref().map(|p| p.space_used).unwrap_or(0), preview.as_ref().map(|p| p.space_cap).unwrap_or(0), preview.as_ref().map(|p| p.cost).unwrap_or(0))
    }

    pub fn browse_len(&self) -> usize {
        self.listings.len() + 1
    }

    pub fn is_new_row(&self) -> bool {
        self.browse_cursor >= self.listings.len()
    }

    pub fn move_browse(&mut self, delta: i32) {
        let len = self.browse_len() as i32;
        let next = (self.browse_cursor as i32 + delta).clamp(0, len - 1);
        self.browse_cursor = next as usize;
    }

    pub fn request_delete_design(&mut self) {
        if self.is_new_row() || self.listings.get(self.browse_cursor).is_some_and(|item| shipyard::STANDARD_CLASS_IDS.contains(&item.design.id.as_str())) {
            self.status = "standard classes are reference-only".into();
            return;
        }
        let index = self.browse_cursor;
        if self.pending == Some(PendingConfirm::DeleteDesign { index }) {
            let listing = self.listings[index].clone();
            let _ = std::fs::remove_file(&listing.path);
            if let Ok(output) = shipyard::generated_path(&self.root, &listing.path) {
                let _ = std::fs::remove_file(output);
            }
            self.pending = None;
            self.refresh_listings();
            self.status = format!("deleted {}", listing.design.name);
        } else {
            self.pending = Some(PendingConfirm::DeleteDesign { index });
            self.status = format!("delete {}? d again to confirm", self.listings[index].design.name);
        }
    }

    pub fn open_selected(&mut self) {
        if self.is_new_row() {
            self.start_new();
            return;
        }
        let listing = &self.listings[self.browse_cursor];
        self.draft = listing.design.clone();
        self.saved = self.draft.clone();
        self.pending = None;
        self.viewing_readonly = shipyard::STANDARD_CLASS_IDS.contains(&self.draft.id.as_str());
        self.edit_cursor = EditField::Name;
        self.edit_scroll = 0;
        self.screen = YardScreen::Edit;
        self.status = format!(
            "editing {} ({}){}",
            self.draft.name,
            listing.path.display(),
            if self.viewing_readonly { " — reference-only standard" } else { "" }
        );
    }

    pub fn clone_selected(&mut self) {
        if self.is_new_row() {
            self.status = "select a saved design to clone".into();
            return;
        }
        let source = self.listings[self.browse_cursor].design.clone();
        let taken_ids: Vec<&str> = self.listings.iter().map(|item| item.design.id.as_str()).collect();
        let taken_names: Vec<&str> = self.listings.iter().map(|item| item.design.name.as_str()).collect();
        let id = shipyard::allocate_id(taken_ids);
        let hull = self.sizes.get(source.size).map(|hull| hull.name.as_str()).unwrap_or("Destroyer");
        let mut draft = source.clone();
        draft.id = id.clone();
        draft.name = shipyard::unique_class_name(hull, taken_names);
        self.draft = draft;
        self.saved = shipyard::new_design(id);
        self.viewing_readonly = false;
        self.pending = None;
        self.edit_cursor = EditField::Name;
        self.edit_scroll = 0;
        self.screen = YardScreen::Edit;
        self.status = format!("cloned {}, type to rename", source.name);
    }

    pub fn start_new(&mut self) {
        let taken_ids: Vec<&str> = self.listings.iter().map(|item| item.design.id.as_str()).collect();
        let taken_names: Vec<&str> = self
            .listings
            .iter()
            .map(|item| item.design.name.as_str())
            .collect();
        let id = shipyard::allocate_id(taken_ids);
        let hull = self
            .sizes
            .get(2)
            .map(|h| h.name.as_str())
            .unwrap_or("Destroyer");
        let name = shipyard::unique_class_name(hull, taken_names);
        self.draft = shipyard::new_design(id);
        self.draft.name = name;
        self.saved = self.draft.clone();
        self.pending = None;
        self.viewing_readonly = false;
        self.edit_cursor = EditField::Name;
        self.edit_scroll = 0;
        self.screen = YardScreen::Edit;
        self.status = "new class — type to rename, ↑/↓ to leave the name".into();
    }

    pub fn is_dirty(&self) -> bool {
        self.draft != self.saved
    }

    /// Esc on the edit screen. Unsaved changes need a second Esc to discard —
    /// the confirming press is `self.pending == Some(DiscardChanges)`, armed
    /// below. Nothing to lose leaves immediately.
    pub fn request_exit(&mut self) {
        if !self.is_dirty() {
            self.pending = None;
            self.back_to_browse();
            return;
        }
        if self.pending == Some(PendingConfirm::DiscardChanges) {
            self.pending = None;
            self.back_to_browse();
        } else {
            self.pending = Some(PendingConfirm::DiscardChanges);
            self.status = "unsaved changes — Esc again to discard, or s to save".into();
        }
    }

    pub fn request_quit(&mut self) -> bool {
        if !self.is_dirty() || self.pending == Some(PendingConfirm::DiscardChanges) {
            return true;
        }
        self.pending = Some(PendingConfirm::DiscardChanges);
        self.status = "unsaved changes, q again to quit or s to save".into();
        false
    }

    /// Clears an armed confirmation without acting on it. Call on any key
    /// that is not the confirming repeat, so an old arm can never fire late
    /// against a since-changed cursor or field.
    pub fn cancel_pending(&mut self) {
        if self.pending.take().is_some() {
            self.status = "cancelled".into();
        }
    }

    fn back_to_browse(&mut self) {
        self.screen = YardScreen::Browse;
        self.viewing_readonly = false;
        self.refresh_listings();
    }

    pub fn field_description(&self) -> String {
        match self.edit_cursor {
            EditField::Name => "Display name used in the class picker and scenarios.".into(),
            EditField::Size => "Hull size sets space, structure, maneuver, and silhouette.".into(),
            EditField::Material => "Material changes structure and the cost multiplier.".into(),
            EditField::Engine => "Choose one of the available engine plants and its thrust step.".into(),
            EditField::Armor => "Armor adds structure for space-free frame cost.".into(),
            EditField::Shields => "Adjust all six shield faces, or press Enter for the shield rosette.".into(),
            EditField::Weapon { index } => {
                let Some(weapon) = self.draft.weapons.get(index) else { return "Weapon row.".into() };
                match self.catalog.weapon(&weapon.component) {
                    Some(spec) => {
                        let mechanic = match spec.kind.as_str() {
                            "beam" => "Beam damage is charge × range factor, strongest at range 1.",
                            "plasma" => "Plasma follows a range table and falls off with distance.",
                            "torp" => "Torpedoes deal flat damage and spend magazine ammo.",
                            "missile" => "Missiles deal flat damage and spend magazine ammo.",
                            "pd" => "Point defense intercepts incoming torpedoes and missiles.",
                            "graviton" => "Graviton ignores shields and armor and hits every ship in the hex.",
                            _ => "Weapon mechanic.",
                        };
                        let tags = spec.quality_tags();
                        if tags.is_empty() {
                            mechanic.into()
                        } else {
                            format!("{mechanic} {}", tags.join(" · "))
                        }
                    }
                    None => "Unknown weapon SKU; choose another component.".into(),
                }
            }
            EditField::System { index } => {
                let Some(system) = self.draft.systems.get(index) else {
                    return "System row.".into();
                };
                match self.catalog.system(&system.component) {
                    Some(spec) => match spec.kind.as_str() {
                        "computer" => format!(
                            "Ship computer: +{} to every to-hit, including PD, at every target size.",
                            spec.mk.unwrap_or(0)
                        ),
                        "cloak" => {
                            "Cloak makes the ship hard to hit (−4) and costs 4+size power this turn.".into()
                        }
                        "repair" => {
                            "Repair spends allocate power to restore hull boxes, capped per turn.".into()
                        }
                        "ecm" => "ECM is always on and subtracts 2 from incoming missile to-hit.".into(),
                        _ => spec.headline(),
                    },
                    None => "Unknown system SKU; choose another component.".into(),
                }
            }
        }
    }

    pub fn weapon_row(&self, weapon: &DesignWeapon) -> String {
        let Some(spec) = self.catalog.weapon(&weapon.component) else {
            return format!("{}   mount {}   unknown SKU", weapon.component, weapon.mount);
        };
        let headline = self.catalog.weapon_headline(&weapon.component);
        let tags = spec.quality_tags();
        if tags.is_empty() {
            format!("{}   mount {}   {headline}", weapon.component, weapon.mount)
        } else {
            format!(
                "{}   mount {}   {headline}   [{}]",
                weapon.component,
                weapon.mount,
                tags.join(", ")
            )
        }
    }

    pub fn system_row(&self, system: &DesignSystem) -> String {
        let Some(spec) = self.catalog.system(&system.component) else {
            return format!("{}   unknown system", system.component);
        };
        format!(
            "{}   {}   {}sp {}c",
            spec.id,
            spec.headline(),
            spec.space,
            spec.cost
        )
    }

    pub fn engine_label(&self) -> String {
        match self.catalog.engine(&self.draft.engine, &self.draft.engine_size) {
            Some(spec) => {
                let step = if spec.thrust_step == 0 {
                    "hull thrust".into()
                } else {
                    format!("thrust{:+}", spec.thrust_step)
                };
                format!(
                    "{} {}   {}pwr {}sp {}c  {step}",
                    spec.kind, spec.size, spec.power, spec.space, spec.cost
                )
            }
            None => format!("{} {}", self.draft.engine, self.draft.engine_size),
        }
    }

    pub fn armor_label(&self) -> String {
        if !self.draft.armored {
            return "no".into();
        }
        let Ok(hull) = self.sizes.get(self.draft.size) else {
            return "yes".into();
        };
        let Ok(mat) = shipyard::material(&self.draft.material) else {
            return "yes".into();
        };
        let extra = ((f64::from(hull.frame_cost) * mat.cost_mult * shipyard::ARMOR_FRAME_COST_MULT)
            + 0.5)
            .floor() as u64;
        format!("yes   1.5× HP  +{extra}c")
    }

    pub fn size_label(&self) -> String {
        match self.sizes.get(self.draft.size) {
            Ok(hull) => format!(
                "{} {}  {}sp {}mv {}hp def{:+}",
                hull.id,
                hull.name,
                hull.space,
                hull.max_maneuver_actions,
                hull.base_structure,
                hull.defense
            ),
            Err(_) => self.draft.size.to_string(),
        }
    }

    pub fn preview(&self) -> Result<DesignPreview, String> {
        shipyard::preview_design(&self.root, &self.draft).map_err(|e| e.to_string())
    }

    pub fn save(&mut self) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        match shipyard::save_design(&self.root, &self.draft) {
            Ok(path) => {
                self.saved = self.draft.clone();
                self.status = format!("saved {}", path.display());
                self.refresh_listings();
            }
            Err(err) => self.status = err.to_string(),
        }
    }

    pub fn compile(&mut self) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        if let Err(err) = shipyard::save_design(&self.root, &self.draft) {
            self.status = err.to_string();
            return;
        }
        self.saved = self.draft.clone();
        let path = self.root.join("data/designs").join(format!("{}.toml", self.draft.id));
        match shipyard::compile(&self.root, &path) {
            Ok(out) => {
                let cost = self
                    .preview()
                    .map(|p| p.cost.to_string())
                    .unwrap_or_else(|_| "?".into());
                self.status = format!("compiled {}  cost {cost}", out.display());
                self.refresh_listings();
            }
            Err(err) => self.status = err.to_string(),
        }
    }

    pub fn nudge(&mut self, delta: i32) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        match self.edit_cursor {
            EditField::Name => {}
            EditField::Size => {
                let prev = self.draft.size;
                let next = (prev as i32 + delta).clamp(1, 7) as u32;
                if next != prev {
                    self.retitle_if_generated(prev, next);
                    self.draft.size = next;
                }
            }
            EditField::Material => {
                let idx = MATERIALS
                    .iter()
                    .position(|m| m.id == self.draft.material)
                    .unwrap_or(0);
                let next = (idx as i32 + delta).rem_euclid(MATERIALS.len() as i32) as usize;
                self.draft.material = MATERIALS[next].id.to_string();
            }
            EditField::Engine => {
                let idx = self.catalog.engines.iter().position(|spec| spec.kind == self.draft.engine && spec.size == self.draft.engine_size).unwrap_or(0);
                let next = (idx as i32 + delta).rem_euclid(self.catalog.engines.len() as i32) as usize;
                let spec = &self.catalog.engines[next];
                self.draft.engine = spec.kind.clone();
                self.draft.engine_size = spec.size.clone();
            }
            EditField::Armor => {
                self.draft.armored = !self.draft.armored;
            }
            EditField::Shields => {
                for face in &mut self.draft.shields {
                    *face = (*face as i64 + i64::from(delta)).clamp(0, 40) as u64;
                }
            }
            EditField::Weapon { index } => {
                if let Some(weapon) = self.draft.weapons.get_mut(index) {
                    if delta.abs() == 1 {
                        cycle_sku(&self.skus, weapon, delta);
                    } else {
                        cycle_mount(weapon, delta.signum());
                    }
                }
            }
            EditField::System { index } => {
                let current = self
                    .draft
                    .systems
                    .get(index)
                    .map(|system| system.component.clone());
                if let Some(current) = current {
                    let choices = self.available_system_skus(Some(&current));
                    if let Some(system) = self.draft.systems.get_mut(index) {
                        cycle_system_sku(&choices, system, delta.signum());
                    }
                }
            }
        }
    }

    pub fn cycle_weapon_mount(&mut self) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        if let EditField::Weapon { index } = self.edit_cursor {
            if let Some(weapon) = self.draft.weapons.get_mut(index) {
                cycle_mount(weapon, 1);
            }
        }
    }

    pub fn add_weapon(&mut self) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        let sku = self.skus.first().cloned().unwrap_or_else(|| "beam".into());
        self.draft.weapons.push(DesignWeapon {
            component: sku,
            mount: "forward".into(),
        });
        self.edit_cursor = EditField::Weapon {
            index: self.draft.weapons.len() - 1,
        };
    }

    pub fn add_system(&mut self) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        let choices = self.available_system_skus(None);
        let Some(sku) = choices
            .iter()
            .find(|id| id.starts_with("computer_"))
            .cloned()
            .or_else(|| choices.into_iter().next())
        else {
            self.status = "already have one computer, cloak, repair, and ECM".into();
            return;
        };
        self.draft.systems.push(DesignSystem { component: sku });
        self.edit_cursor = EditField::System {
            index: self.draft.systems.len() - 1,
        };
        self.status = "added system — ←/→ change mark or type".into();
    }

    pub fn set_shield_face(&mut self, index: usize, value: u64) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        if let Some(face) = self.draft.shields.get_mut(index) {
            *face = value.min(40);
        }
    }

    fn available_system_skus(&self, keep: Option<&str>) -> Vec<String> {
        let keep_kind = keep.and_then(|id| {
            self.catalog.system(id).map(|spec| spec.kind.clone())
        });
        let used: Vec<String> = self
            .draft
            .systems
            .iter()
            .filter_map(|system| {
                if keep.is_some_and(|id| id == system.component) {
                    return None;
                }
                self.catalog.system(&system.component).map(|spec| spec.kind.clone())
            })
            .collect();
        self.system_skus
            .iter()
            .filter(|id| {
                let Some(spec) = self.catalog.system(id) else {
                    return false;
                };
                keep_kind.as_deref() == Some(spec.kind.as_str()) || !used.contains(&spec.kind)
            })
            .cloned()
            .collect()
    }

    /// 'd' on a weapon or system row. First press arms it; the same key on the
    /// same row confirms. Any other key cancels (`cancel_pending`).
    pub fn request_delete_weapon(&mut self) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        if let EditField::System { index } = self.edit_cursor {
            self.request_delete_system(index);
            return;
        }
        let EditField::Weapon { index } = self.edit_cursor else {
            return;
        };
        if self.draft.weapons.len() <= 1 {
            self.status = "a ship needs at least one weapon".into();
            return;
        }
        if self.pending == Some(PendingConfirm::DeleteWeapon { index }) {
            self.pending = None;
            let sku = self.draft.weapons[index].component.clone();
            self.draft.weapons.remove(index);
            self.edit_cursor = EditField::Weapon {
                index: index.min(self.draft.weapons.len() - 1),
            };
            self.status = format!("deleted {sku}");
        } else {
            self.pending = Some(PendingConfirm::DeleteWeapon { index });
            let sku = &self.draft.weapons[index].component;
            self.status = format!("delete {sku}? d again to confirm, any other key cancels");
        }
    }

    fn request_delete_system(&mut self, index: usize) {
        if index >= self.draft.systems.len() {
            return;
        }
        if self.pending == Some(PendingConfirm::DeleteSystem { index }) {
            self.pending = None;
            let sku = self.draft.systems[index].component.clone();
            self.draft.systems.remove(index);
            self.edit_cursor = if self.draft.systems.is_empty() {
                EditField::Weapon {
                    index: self.draft.weapons.len().saturating_sub(1),
                }
            } else {
                EditField::System {
                    index: index.min(self.draft.systems.len() - 1),
                }
            };
            self.status = format!("deleted {sku}");
        } else {
            self.pending = Some(PendingConfirm::DeleteSystem { index });
            let sku = &self.draft.systems[index].component;
            self.status = format!("delete {sku}? d again to confirm, any other key cancels");
        }
    }

    pub fn move_edit(&mut self, delta: i32) {
        let fields = self.field_list();
        let idx = fields
            .iter()
            .position(|f| *f == self.edit_cursor)
            .unwrap_or(0);
        let next = (idx as i32 + delta).clamp(0, fields.len() as i32 - 1) as usize;
        self.edit_cursor = fields[next];
    }

    pub fn field_list(&self) -> Vec<EditField> {
        let mut fields = vec![
            EditField::Name,
            EditField::Size,
            EditField::Material,
            EditField::Engine,
            EditField::Armor,
            EditField::Shields,
        ];
        for index in 0..self.draft.weapons.len() {
            fields.push(EditField::Weapon { index });
        }
        for index in 0..self.draft.systems.len() {
            fields.push(EditField::System { index });
        }
        fields
    }

    pub fn edit_rows(&self) -> Vec<YardRow> {
        let mut rows = vec![
            YardRow { kind: YardRowKind::Field(EditField::Name), text: format!("name            {}", self.draft.name) },
            YardRow { kind: YardRowKind::Field(EditField::Size), text: format!("size            {}", self.size_label()) },
            YardRow { kind: YardRowKind::Field(EditField::Material), text: format!("material        {}", material_label(&self.draft.material)) },
            YardRow { kind: YardRowKind::Field(EditField::Engine), text: format!("engine          {}", self.engine_label()) },
            YardRow { kind: YardRowKind::Field(EditField::Armor), text: format!("armor           {}", self.armor_label()) },
            YardRow { kind: YardRowKind::Field(EditField::Shields), text: format!("shields         {} banks   {}sp  {}c   <-/-> every face", self.draft.shields.iter().sum::<u64>(), self.draft.shields.iter().sum::<u64>(), self.draft.shields.iter().sum::<u64>()) },
        ];
        rows.push(YardRow { kind: YardRowKind::Blank, text: String::new() });
        rows.push(YardRow { kind: YardRowKind::Section, text: "weapons  (left/right sku   m mount   a add   d delete)".into() });
        for (index, weapon) in self.draft.weapons.iter().enumerate() {
            rows.push(YardRow { kind: YardRowKind::Field(EditField::Weapon { index }), text: self.weapon_row(weapon) });
        }
        rows.push(YardRow { kind: YardRowKind::Blank, text: String::new() });
        rows.push(YardRow { kind: YardRowKind::Section, text: "systems  (i add   left/right change   d delete)".into() });
        if self.draft.systems.is_empty() {
            rows.push(YardRow { kind: YardRowKind::Blank, text: "  (none - i to install a computer for accuracy)".into() });
        }
        for (index, system) in self.draft.systems.iter().enumerate() {
            rows.push(YardRow { kind: YardRowKind::Field(EditField::System { index }), text: self.system_row(system) });
        }
        rows
    }

    fn retitle_if_generated(&mut self, _old_size: u32, new_size: u32) {
        let names: Vec<String> = self.sizes.sizes.iter().map(|h| h.name.clone()).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        if !shipyard::is_generated_class_name(&self.draft.name, &refs) {
            return;
        }
        let Some(hull) = self.sizes.get(new_size).ok() else {
            return;
        };
        let taken: Vec<&str> = self
            .listings
            .iter()
            .filter(|item| item.design.id != self.draft.id)
            .map(|item| item.design.name.as_str())
            .collect();
        self.draft.name = shipyard::unique_class_name(&hull.name, taken);
    }

    pub fn type_name(&mut self, ch: char) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        if self.draft.name.len() >= 40 {
            return;
        }
        let ok = ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '-' | '\'' | '.');
        if !ok {
            return;
        }
        if ch == ' ' && self.draft.name.ends_with(' ') {
            return;
        }
        self.draft.name.push(ch);
    }

    pub fn backspace_name(&mut self) {
        if self.viewing_readonly {
            self.status = "standard classes are reference-only".into();
            return;
        }
        self.draft.name.pop();
    }
}

pub fn material_label(id: &str) -> String {
    match MATERIALS.iter().find(|m| m.id == id) {
        Some(m) => format!("{}  (tech {}, {:.1}× HP)", m.name, m.tech, m.structure_mult),
        None => id.to_string(),
    }
}

fn cycle_system_sku(skus: &[String], system: &mut DesignSystem, delta: i32) {
    if skus.is_empty() {
        return;
    }
    let idx = skus
        .iter()
        .position(|s| s == &system.component)
        .unwrap_or(0);
    let next = (idx as i32 + delta).rem_euclid(skus.len() as i32) as usize;
    system.component = skus[next].clone();
}

fn cycle_sku(skus: &[String], weapon: &mut DesignWeapon, delta: i32) {
    if skus.is_empty() {
        return;
    }
    let idx = skus
        .iter()
        .position(|s| s == &weapon.component)
        .unwrap_or(0);
    let next = (idx as i32 + delta).rem_euclid(skus.len() as i32) as usize;
    weapon.component = skus[next].clone();
}

fn cycle_mount(weapon: &mut DesignWeapon, delta: i32) {
    let idx = MOUNTS
        .iter()
        .position(|m| *m == weapon.mount)
        .unwrap_or(0);
    let next = (idx as i32 + delta).rem_euclid(MOUNTS.len() as i32) as usize;
    weapon.mount = MOUNTS[next].to_string();
}

fn metadata_time(path: &Path) -> SystemTime {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn preview_cost(preview: &Result<DesignPreview, String>) -> u64 {
    preview
        .as_ref()
        .map(|p| u64::from(p.cost))
        .unwrap_or(u64::MAX)
}

pub fn find_repo_root() -> PathBuf {
    if let Ok(root) = std::env::var("SHIPSIM_ROOT") {
        return PathBuf::from(root);
    }
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir.pop();
    if dir.join("data/designs").is_dir() {
        return dir;
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn repo_has_yard(root: &Path) -> bool {
    root.join("data/designs").is_dir() && root.join("data/components.toml").is_file()
}
