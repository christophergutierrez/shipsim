//! In-process shipyard: browse existing designs or create/edit one.
//! Cost and space come from `shipsim_core::shipyard`, not from combat.

use std::path::{Path, PathBuf};

use shipsim_core::shipyard::{
    self, Design, DesignPreview, DesignWeapon, ENGINE_KINDS, ENGINE_SIZES, MATERIALS, MOUNTS,
};
use shipsim_core::sizes::{self, SizeTable};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YardScreen {
    Browse,
    Edit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditField {
    Name,
    Size,
    Material,
    EngineKind,
    EngineSize,
    Armor,
    ShieldsAll,
    ShieldsFace { index: usize },
    Weapon { index: usize },
}

/// An action armed by one keypress, awaiting a second confirming keypress.
/// Any other key cancels it (see `input.rs`) rather than leaving it armed
/// indefinitely, so a later unrelated `d` can never land on a stale arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingConfirm {
    DiscardChanges,
    DeleteWeapon { index: usize },
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
    pub sizes: SizeTable,
    pub pending: Option<PendingConfirm>,
}

impl YardState {
    pub fn load(root: PathBuf) -> Result<Self, String> {
        let skus = shipyard::weapon_skus(&root).map_err(|e| e.to_string())?;
        let sizes = sizes::load(&root).map_err(|e| e.to_string())?;
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
            sizes,
            pending: None,
        };
        yard.refresh_listings();
        Ok(yard)
    }

    pub fn refresh_listings(&mut self) {
        match shipyard::list_designs(&self.root) {
            Ok(list) => {
                self.listings = list
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
                    .collect();
                if self.browse_cursor > self.listings.len() {
                    self.browse_cursor = self.listings.len();
                }
                self.status = format!("{} saved design(s) — Enter edit · n new", self.listings.len());
            }
            Err(err) => {
                self.listings.clear();
                self.status = err.to_string();
            }
        }
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

    pub fn open_selected(&mut self) {
        if self.is_new_row() {
            self.start_new();
            return;
        }
        let listing = &self.listings[self.browse_cursor];
        self.draft = listing.design.clone();
        self.saved = self.draft.clone();
        self.pending = None;
        self.edit_cursor = EditField::Name;
        self.screen = YardScreen::Edit;
        self.status = format!(
            "editing {} ({})",
            self.draft.name,
            listing.path.display()
        );
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
        self.edit_cursor = EditField::Name;
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
        self.refresh_listings();
    }

    pub fn engine_label(&self) -> String {
        match shipyard::engine_spec(&self.root, &self.draft.engine, &self.draft.engine_size) {
            Ok(spec) => {
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
            Err(_) => format!("{} {}", self.draft.engine, self.draft.engine_size),
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
            EditField::EngineKind => {
                let idx = ENGINE_KINDS
                    .iter()
                    .position(|k| *k == self.draft.engine)
                    .unwrap_or(0);
                let next = (idx as i32 + delta).rem_euclid(ENGINE_KINDS.len() as i32) as usize;
                self.draft.engine = ENGINE_KINDS[next].to_string();
            }
            EditField::EngineSize => {
                let idx = ENGINE_SIZES
                    .iter()
                    .position(|s| *s == self.draft.engine_size)
                    .unwrap_or(0);
                let next = (idx as i32 + delta).rem_euclid(ENGINE_SIZES.len() as i32) as usize;
                self.draft.engine_size = ENGINE_SIZES[next].to_string();
            }
            EditField::Armor => {
                self.draft.armored = !self.draft.armored;
            }
            EditField::ShieldsAll => {
                for face in &mut self.draft.shields {
                    *face = (*face as i64 + i64::from(delta)).clamp(0, 40) as u64;
                }
            }
            EditField::ShieldsFace { index } => {
                if let Some(face) = self.draft.shields.get_mut(index) {
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
        }
    }

    pub fn cycle_weapon_mount(&mut self) {
        if let EditField::Weapon { index } = self.edit_cursor {
            if let Some(weapon) = self.draft.weapons.get_mut(index) {
                cycle_mount(weapon, 1);
            }
        }
    }

    pub fn add_weapon(&mut self) {
        let sku = self.skus.first().cloned().unwrap_or_else(|| "beam".into());
        self.draft.weapons.push(DesignWeapon {
            component: sku,
            mount: "forward".into(),
        });
        self.edit_cursor = EditField::Weapon {
            index: self.draft.weapons.len() - 1,
        };
    }

    /// 'd' on a weapon row. First press arms it (status names the weapon and
    /// asks for a repeat); pressing 'd' again on the *same* weapon confirms.
    /// Moving off the row or pressing any other key cancels the arm
    /// (`cancel_pending`, called from `input.rs`) rather than leaving a
    /// dangling confirmation that could later delete the wrong row.
    pub fn request_delete_weapon(&mut self) {
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

    pub fn move_edit(&mut self, delta: i32) {
        let fields = self.field_list();
        let idx = fields
            .iter()
            .position(|f| *f == self.edit_cursor)
            .unwrap_or(0);
        let next = (idx as i32 + delta).clamp(0, fields.len() as i32 - 1) as usize;
        self.edit_cursor = fields[next];
    }

    fn field_list(&self) -> Vec<EditField> {
        let mut fields = vec![
            EditField::Name,
            EditField::Size,
            EditField::Material,
            EditField::EngineKind,
            EditField::EngineSize,
            EditField::Armor,
            EditField::ShieldsAll,
        ];
        for index in 0..6 {
            fields.push(EditField::ShieldsFace { index });
        }
        for index in 0..self.draft.weapons.len() {
            fields.push(EditField::Weapon { index });
        }
        fields
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
        self.draft.name.pop();
    }
}

pub fn material_label(id: &str) -> String {
    match MATERIALS.iter().find(|m| m.id == id) {
        Some(m) => format!("{}  (tech {}, {:.1}× HP)", m.name, m.tech, m.structure_mult),
        None => id.to_string(),
    }
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
