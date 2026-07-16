//! Validated, immutable global rules loaded alongside scenario content.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use thiserror::Error;

use crate::combat_tables::WeaponKind;
use crate::ssd::DacSlot;

const DEFAULT_RULES_TEXT: &str = include_str!("../data/rules/default.toml");

#[derive(Debug, Clone)]
pub struct Ruleset {
    schema_version: u32,
    id: String,
    combat: CombatRules,
    ssd: SsdRules,
    fingerprint: String,
    dac_slots: Vec<DacSlot>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRules {
    die_sides: u8,
    accuracy: AccuracyRules,
    weapons: WeaponRules,
}

impl CombatRules {
    pub fn die_sides(&self) -> u8 {
        self.die_sides
    }

    pub fn accuracy(&self) -> &AccuracyRules {
        &self.accuracy
    }

    pub fn weapons(&self) -> &WeaponRules {
        &self.weapons
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccuracyRules {
    baseline_target_size: u32,
    size_multiplier: SizeMultiplier,
    ceiling_floor: u8,
    ceiling_max: u8,
    fire_control_target_size: u32,
}

impl AccuracyRules {
    pub fn baseline_target_size(&self) -> u32 {
        self.baseline_target_size
    }

    pub fn size_multiplier(&self) -> SizeMultiplier {
        self.size_multiplier
    }

    pub fn ceiling_floor(&self) -> u8 {
        self.ceiling_floor
    }

    pub fn ceiling_max(&self) -> u8 {
        self.ceiling_max
    }

    pub fn fire_control_target_size(&self) -> u32 {
        self.fire_control_target_size
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeMultiplier {
    TargetOverBaseline,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponRules {
    beam: BeamRules,
    plasma: PlasmaRules,
    torp: TorpRules,
}

impl WeaponRules {
    pub fn beam(&self) -> &BeamRules {
        &self.beam
    }

    pub fn plasma(&self) -> &PlasmaRules {
        &self.plasma
    }

    pub fn torp(&self) -> &TorpRules {
        &self.torp
    }
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct BeamRules {
    to_hit: Vec<u8>,
    damage_model: BeamDamageModel,
    range_factors: Vec<f64>,
}

impl BeamRules {
    pub fn to_hit(&self) -> &[u8] {
        &self.to_hit
    }

    pub fn damage_model(&self) -> BeamDamageModel {
        self.damage_model
    }

    pub fn range_factors(&self) -> &[f64] {
        &self.range_factors
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BeamDamageModel {
    ChargeTimesRangeFactor,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlasmaRules {
    to_hit: Vec<u8>,
    damage_model: RangeDamageModel,
    damage: Vec<u32>,
}

impl PlasmaRules {
    pub fn to_hit(&self) -> &[u8] {
        &self.to_hit
    }

    pub fn damage_model(&self) -> RangeDamageModel {
        self.damage_model
    }

    pub fn damage(&self) -> &[u32] {
        &self.damage
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RangeDamageModel {
    RangeTable,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TorpRules {
    to_hit: Vec<u8>,
    damage_model: TorpDamageModel,
    flat_damage: u32,
}

impl TorpRules {
    pub fn to_hit(&self) -> &[u8] {
        &self.to_hit
    }

    pub fn damage_model(&self) -> TorpDamageModel {
        self.damage_model
    }

    pub fn flat_damage(&self) -> u32 {
        self.flat_damage
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TorpDamageModel {
    Flat,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct SsdRules {
    dac: Vec<DacSlotName>,
}

#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DacSlotName {
    Hull,
    Engine,
    Power,
    Weapon,
}

#[derive(Debug, Error)]
pub enum RulesError {
    #[error("cannot read rules {path:?}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse rules {path:?}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("rules schema version {actual} is unsupported; expected {expected}")]
    UnsupportedSchema { actual: u32, expected: u32 },
    #[error("rules validation failed: {0}")]
    Invalid(String),
}

impl Ruleset {
    pub fn load(data_root: &Path) -> Result<Arc<Self>, RulesError> {
        let path = data_root.join("data").join("rules").join("default.toml");
        let text = std::fs::read_to_string(&path).map_err(|source| RulesError::Read {
            path: path.clone(),
            source,
        })?;
        Self::from_text(&path, &text).map(Arc::new)
    }

    pub fn builtin() -> Arc<Self> {
        Self::from_text(Path::new("data/rules/default.toml"), DEFAULT_RULES_TEXT)
            .expect("built-in rules must validate")
            .into()
    }

    pub fn from_text(path: &Path, text: &str) -> Result<Self, RulesError> {
        let raw: RawRuleset = toml::from_str(text).map_err(|source| RulesError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        if raw.schema_version != 1 {
            return Err(RulesError::UnsupportedSchema {
                actual: raw.schema_version,
                expected: 1,
            });
        }
        let fingerprint = fingerprint(
            &serde_json::to_vec(&RawRuleset {
                schema_version: raw.schema_version,
                id: raw.id.clone(),
                combat: raw.combat.clone(),
                ssd: raw.ssd.clone(),
            })
            .expect("rules serialization"),
        );
        let rules = Self {
            schema_version: raw.schema_version,
            id: raw.id,
            combat: raw.combat,
            ssd: raw.ssd,
            fingerprint,
            dac_slots: Vec::new(),
        };
        rules.validate()?;
        let mut rules = rules;
        rules.dac_slots = rules
            .ssd
            .dac
            .iter()
            .map(|slot| match slot {
                DacSlotName::Hull => DacSlot::Hull,
                DacSlotName::Engine => DacSlot::Engine,
                DacSlotName::Power => DacSlot::Power,
                DacSlotName::Weapon => DacSlot::Weapon,
            })
            .collect();
        Ok(rules)
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn combat(&self) -> &CombatRules {
        &self.combat
    }

    pub fn ssd_rules(&self) -> &SsdRules {
        &self.ssd
    }

    pub fn validate(&self) -> Result<(), RulesError> {
        // Extraction plan (data/rules/default.toml, ADR-0024) deferred alternate
        // dice systems; schema version 1 currently supports d20 only.
        if self.combat.die_sides != 20 {
            return Err(RulesError::Invalid(format!(
                "combat.die_sides must be 20; schema version 1 supports only d20 rules (got {})",
                self.combat.die_sides
            )));
        }
        let combat = &self.combat;
        let accuracy = &combat.accuracy;
        if accuracy.baseline_target_size == 0 {
            return Err(RulesError::Invalid(
                "combat.accuracy.baseline_target_size must be positive".into(),
            ));
        }
        if accuracy.fire_control_target_size == 0 {
            return Err(RulesError::Invalid(
                "combat.accuracy.fire_control_target_size must be positive".into(),
            ));
        }
        if accuracy.ceiling_floor > accuracy.ceiling_max {
            return Err(RulesError::Invalid(
                "accuracy.ceiling_floor cannot exceed ceiling_max".into(),
            ));
        }
        if accuracy.ceiling_max >= combat.die_sides {
            return Err(RulesError::Invalid(
                "accuracy.ceiling_max must be below combat.die_sides".into(),
            ));
        }

        let beam = &combat.weapons.beam;
        if beam.to_hit.is_empty() || beam.to_hit.len() != beam.range_factors.len() {
            return Err(RulesError::Invalid(
                "beam to_hit and range_factors must be non-empty and equal length".into(),
            ));
        }
        if beam
            .range_factors
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return Err(RulesError::Invalid(
                "beam range_factors must be finite and positive".into(),
            ));
        }

        let plasma = &combat.weapons.plasma;
        if plasma.to_hit.is_empty() || plasma.to_hit.len() != plasma.damage.len() {
            return Err(RulesError::Invalid(
                "plasma to_hit and damage must be non-empty and equal length".into(),
            ));
        }
        if plasma.damage.contains(&0) {
            return Err(RulesError::Invalid("plasma damage must be positive".into()));
        }

        let torp = &combat.weapons.torp;
        if torp.to_hit.is_empty() || torp.flat_damage == 0 {
            return Err(RulesError::Invalid(
                "torp to_hit and flat_damage must be positive".into(),
            ));
        }
        for (name, values) in [
            ("beam", beam.to_hit.as_slice()),
            ("plasma", plasma.to_hit.as_slice()),
            ("torp", torp.to_hit.as_slice()),
        ] {
            if values.contains(&0) || values.iter().any(|value| *value > combat.die_sides) {
                return Err(RulesError::Invalid(format!(
                    "{name} to_hit values must be in 1..={}",
                    combat.die_sides
                )));
            }
        }
        if self.ssd.dac.is_empty() {
            return Err(RulesError::Invalid("ssd.dac must not be empty".into()));
        }
        Ok(())
    }

    pub fn max_range(&self, kind: WeaponKind) -> u32 {
        match kind {
            WeaponKind::Beam => self.combat.weapons.beam.to_hit.len() as u32,
            WeaponKind::Plasma => self.combat.weapons.plasma.to_hit.len() as u32,
            WeaponKind::Torp => self.combat.weapons.torp.to_hit.len() as u32,
        }
    }

    pub fn dac(&self) -> &[DacSlot] {
        &self.dac_slots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text() -> String {
        DEFAULT_RULES_TEXT.to_string()
    }

    #[test]
    fn builtin_rules_have_expected_identity_and_tables() {
        let rules = Ruleset::builtin();
        assert_eq!(rules.schema_version(), 1);
        assert_eq!(rules.id(), "default");
        assert_eq!(rules.max_range(WeaponKind::Beam), 10);
        assert_eq!(rules.max_range(WeaponKind::Plasma), 14);
        assert_eq!(rules.max_range(WeaponKind::Torp), 12);
        assert_eq!(rules.dac().len(), 16);
        assert!(rules.fingerprint().starts_with("fnv1a-"));
    }

    #[test]
    fn fingerprint_is_stable_across_toml_whitespace() {
        let compact = Ruleset::from_text(Path::new("compact.toml"), DEFAULT_RULES_TEXT)
            .expect("compact rules");
        let padded = Ruleset::from_text(
            Path::new("padded.toml"),
            &format!("\n{}\n", DEFAULT_RULES_TEXT),
        )
        .expect("padded rules");
        assert_eq!(compact.fingerprint(), padded.fingerprint());
    }

    #[test]
    fn invalid_rules_are_rejected() {
        let cases = [
            ("die_sides = 20", "die_sides = 0", "die sides"),
            ("die_sides = 20", "die_sides = 12", "die sides not 20"),
            (
                "baseline_target_size = 2",
                "baseline_target_size = 0",
                "baseline",
            ),
            (
                "ceiling_floor = 15",
                "ceiling_floor = 20",
                "ceiling floor",
            ),
            (
                "dac = [\n  \"hull\", \"hull\", \"engine\", \"weapon\",\n  \"hull\", \"power\", \"weapon\", \"hull\",\n  \"hull\", \"engine\", \"hull\", \"weapon\",\n  \"power\", \"hull\", \"hull\", \"engine\",\n]",
                "dac = []",
                "DAC",
            ),
        ];
        for (from, to, label) in cases {
            let error = Ruleset::from_text(Path::new("test.toml"), &text().replace(from, to))
                .expect_err(label);
            assert!(error.to_string().contains("rules"), "{label}: {error}");
        }
    }

    #[test]
    fn only_d20_is_supported_by_schema_version_one() {
        let error = Ruleset::from_text(
            Path::new("test.toml"),
            &text().replace("die_sides = 20", "die_sides = 12"),
        )
        .expect_err("d12 must be rejected");
        assert!(error.to_string().contains("d20"), "{error}");
    }

    #[test]
    fn non_finite_or_non_positive_beam_range_factors_are_rejected() {
        for (replacement, label) in [
            ("[2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, nan]", "NaN"),
            ("[2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, inf]", "inf"),
            (
                "[2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, -inf]",
                "-inf",
            ),
            ("[2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, 0.0]", "zero"),
            (
                "[2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, -1.0]",
                "negative",
            ),
        ] {
            let modified = text().replace(
                "range_factors = [2.0, 1.9, 1.7, 1.6, 1.4, 1.3, 1.2, 1.1, 1.0, 1.0]",
                &format!("range_factors = {replacement}"),
            );
            let error = Ruleset::from_text(Path::new("test.toml"), &modified).expect_err(label);
            assert!(
                error.to_string().contains("range_factors"),
                "{label}: {error}"
            );
        }
    }

    #[test]
    fn disk_loaded_and_embedded_default_rules_share_identity_and_fingerprint() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let disk = Ruleset::load(root).expect("repository rules load from disk");
        let embedded = Ruleset::builtin();
        assert_eq!(disk.id(), embedded.id());
        assert_eq!(disk.fingerprint(), embedded.fingerprint());
    }

    #[test]
    fn unknown_fields_and_models_are_rejected() {
        let unknown = format!("{}\nunknown = true\n", text());
        assert!(matches!(
            Ruleset::from_text(Path::new("test.toml"), &unknown),
            Err(RulesError::Parse { .. })
        ));
        let model = text().replace("damage_model = \"flat\"", "damage_model = \"burst\"");
        assert!(matches!(
            Ruleset::from_text(Path::new("test.toml"), &model),
            Err(RulesError::Parse { .. })
        ));
    }
}

#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct RawRuleset {
    schema_version: u32,
    id: String,
    combat: CombatRules,
    ssd: SsdRules,
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a-{hash:016x}")
}
