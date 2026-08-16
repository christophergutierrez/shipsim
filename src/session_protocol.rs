//! Typed wire contract for pre-match session and lobby negotiation.
//!
//! Session protocol v1 is deliberately separate from game protocol v4. This
//! module defines data and validation only; session-server runtime behavior is
//! introduced in a later milestone.

use std::collections::{btree_map::Entry, BTreeMap};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::PROTOCOL_VERSION;
use crate::schema::SideId;

pub const SESSION_PROTOCOL_VERSION: u32 = 1;
pub const SUPPORTED_SESSION_PROTOCOL_VERSIONS: &[u32] = &[SESSION_PROTOCOL_VERSION];
pub const SUPPORTED_GAME_PROTOCOL_VERSIONS: &[u32] = &[PROTOCOL_VERSION];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Tui,
    Repl,
    Love2d,
    Agent,
    Test,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantStatus {
    Ready,
    Thinking,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControllerSpec {
    Human {},
    Bot { policy: String },
    LlmAgent {},
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerAssignments {
    pub a: ControllerSpec,
    pub b: ControllerSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LobbyPhase {
    Unconfigured,
    Configured,
    WaitingForSeats,
    Running,
    Finished,
}

/// Connection/lobby lifecycle frozen by session protocol v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Disconnected,
    Connected,
    HelloNegotiated,
    LobbyUnconfigured,
    LobbyConfigured,
    WaitingForSeats,
    Running,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    Connect,
    Hello,
    OpenLobby,
    CreateMatch,
    WaitForSeats,
    SeatsReady,
    Finish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionStateMachine {
    state: SessionState,
}

impl Default for SessionStateMachine {
    fn default() -> Self {
        Self::at(SessionState::Disconnected)
    }
}

impl SessionStateMachine {
    pub fn at(state: SessionState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn apply(&mut self, event: SessionEvent) -> Result<SessionState, SessionProtocolError> {
        let next = match (self.state, event) {
            (SessionState::Disconnected, SessionEvent::Connect) => SessionState::Connected,
            (SessionState::Connected, SessionEvent::Hello) => SessionState::HelloNegotiated,
            (SessionState::HelloNegotiated, SessionEvent::OpenLobby) => {
                SessionState::LobbyUnconfigured
            }
            (SessionState::LobbyUnconfigured, SessionEvent::CreateMatch) => {
                SessionState::LobbyConfigured
            }
            (SessionState::LobbyConfigured, SessionEvent::WaitForSeats) => {
                SessionState::WaitingForSeats
            }
            (SessionState::WaitingForSeats, SessionEvent::SeatsReady) => SessionState::Running,
            (SessionState::Running, SessionEvent::Finish) => SessionState::Finished,
            (state, event) => {
                return Err(SessionProtocolError {
                    code: SessionErrorCode::InvalidState,
                    message: format!("event {event:?} is not valid in state {state:?}"),
                });
            }
        };
        self.state = next;
        Ok(next)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeatOccupancy {
    Vacant,
    Reserved,
    Occupied,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatLobbyState {
    pub side: SideId,
    pub controller: ControllerSpec,
    pub occupancy: SeatOccupancy,
    pub display_name: Option<String>,
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant_status: Option<ParticipantStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioSummary {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("cannot read scenario catalog {path:?}: {source}")]
    ReadCatalog {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot parse scenario catalog {path:?}: {source}")]
    ParseCatalog {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("unsupported scenario catalog schema {actual}; expected 1")]
    UnsupportedSchema { actual: u32 },
    #[error("invalid scenario identifier {identifier:?}")]
    InvalidIdentifier { identifier: String },
    #[error("duplicate scenario identifier {identifier:?}")]
    DuplicateIdentifier { identifier: String },
    #[error("scenario {identifier:?} has unsafe catalog path {path:?}")]
    UnsafeCatalogPath { identifier: String, path: PathBuf },
    #[error("cannot resolve scenario directory {path:?}: {source}")]
    ScenarioDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot resolve scenario {identifier:?} at {path:?}: {source}")]
    ScenarioFile {
        identifier: String,
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("scenario {identifier:?} resolves outside the scenario directory: {path:?}")]
    ScenarioPathEscape { identifier: String, path: PathBuf },
    #[error("scenario {identifier:?} does not resolve to a regular file: {path:?}")]
    ScenarioNotFile { identifier: String, path: PathBuf },
    #[error("unknown scenario {identifier:?}")]
    UnknownScenario { identifier: String },
    #[error("scenario {identifier:?} is disabled for network play")]
    DisabledScenario { identifier: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioCatalogFile {
    schema_version: u32,
    scenarios: Vec<ScenarioCatalogFileEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioCatalogFileEntry {
    id: String,
    display_name: String,
    path: PathBuf,
    network_enabled: bool,
}

#[derive(Debug)]
struct ScenarioCatalogEntry {
    summary: ScenarioSummary,
    path: PathBuf,
    network_enabled: bool,
}

/// Server-owned scenario allowlist. Filesystem paths are intentionally private
/// and are never included in the public projection.
#[derive(Debug)]
pub struct ScenarioCatalog {
    entries: BTreeMap<String, ScenarioCatalogEntry>,
}

impl ScenarioCatalog {
    pub fn load(repository_root: &Path) -> Result<Self, CatalogError> {
        Self::load_from_paths(
            &repository_root.join("data/scenario_catalog.toml"),
            &repository_root.join("scenarios"),
        )
    }

    pub fn load_from_paths(
        catalog_path: &Path,
        scenario_directory: &Path,
    ) -> Result<Self, CatalogError> {
        let text =
            std::fs::read_to_string(catalog_path).map_err(|source| CatalogError::ReadCatalog {
                path: catalog_path.to_path_buf(),
                source,
            })?;
        let file: ScenarioCatalogFile =
            toml::from_str(&text).map_err(|source| CatalogError::ParseCatalog {
                path: catalog_path.to_path_buf(),
                source,
            })?;
        if file.schema_version != 1 {
            return Err(CatalogError::UnsupportedSchema {
                actual: file.schema_version,
            });
        }

        let canonical_directory = scenario_directory.canonicalize().map_err(|source| {
            CatalogError::ScenarioDirectory {
                path: scenario_directory.to_path_buf(),
                source,
            }
        })?;
        let mut entries = BTreeMap::new();
        for configured in file.scenarios {
            validate_scenario_identifier(&configured.id)?;
            validate_catalog_path(&configured.id, &configured.path)?;
            let joined = canonical_directory.join(&configured.path);
            let canonical_path =
                joined
                    .canonicalize()
                    .map_err(|source| CatalogError::ScenarioFile {
                        identifier: configured.id.clone(),
                        path: joined.clone(),
                        source,
                    })?;
            if !canonical_path.starts_with(&canonical_directory) {
                return Err(CatalogError::ScenarioPathEscape {
                    identifier: configured.id,
                    path: canonical_path,
                });
            }
            if !canonical_path.is_file() {
                return Err(CatalogError::ScenarioNotFile {
                    identifier: configured.id,
                    path: canonical_path,
                });
            }
            let id = configured.id;
            let entry = ScenarioCatalogEntry {
                summary: ScenarioSummary {
                    id: id.clone(),
                    display_name: configured.display_name,
                },
                path: canonical_path,
                network_enabled: configured.network_enabled,
            };
            match entries.entry(id.clone()) {
                Entry::Vacant(slot) => {
                    slot.insert(entry);
                }
                Entry::Occupied(_) => {
                    return Err(CatalogError::DuplicateIdentifier { identifier: id });
                }
            }
        }
        Ok(Self { entries })
    }

    pub fn projection(&self) -> Vec<ScenarioSummary> {
        self.entries
            .values()
            .filter(|entry| entry.network_enabled)
            .map(|entry| entry.summary.clone())
            .collect()
    }

    pub fn resolve(&self, identifier: &str) -> Result<&Path, CatalogError> {
        validate_scenario_identifier(identifier)?;
        let entry = self
            .entries
            .get(identifier)
            .ok_or_else(|| CatalogError::UnknownScenario {
                identifier: identifier.to_string(),
            })?;
        if !entry.network_enabled {
            return Err(CatalogError::DisabledScenario {
                identifier: identifier.to_string(),
            });
        }
        Ok(&entry.path)
    }
}

fn validate_scenario_identifier(identifier: &str) -> Result<(), CatalogError> {
    let valid = !identifier.is_empty()
        && identifier.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        });
    if valid {
        Ok(())
    } else {
        Err(CatalogError::InvalidIdentifier {
            identifier: identifier.to_string(),
        })
    }
}

fn validate_catalog_path(identifier: &str, path: &Path) -> Result<(), CatalogError> {
    let components: Vec<_> = path.components().collect();
    let safe = !path.is_absolute()
        && components.len() == 1
        && matches!(components[0], Component::Normal(_))
        && path.extension().and_then(|extension| extension.to_str()) == Some("toml")
        && !path.to_string_lossy().contains(['/', '\\']);
    if safe {
        Ok(())
    } else {
        Err(CatalogError::UnsafeCatalogPath {
            identifier: identifier.to_string(),
            path: path.to_path_buf(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BotPolicySummary {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerCapability {
    ScenarioCatalog,
    PreviewRequests,
    ParticipantStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionErrorCode {
    UnsupportedSessionVersion,
    UnsupportedGameProtocol,
    HostRequired,
    LobbyAlreadyConfigured,
    UnknownScenario,
    UnknownController,
    UnknownBotPolicy,
    SeatNotReady,
    InvalidJoinToken,
    SeatAlreadyOccupied,
    Ownership,
    RequestRejected,
    InvalidState,
    InvalidMessage,
    ParticipantDisconnected,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code:?}: {message}")]
pub struct SessionProtocolError {
    pub code: SessionErrorCode,
    pub message: String,
}

impl SessionProtocolError {
    fn unsupported(code: SessionErrorCode, offered: &[u32], supported: &[u32]) -> Self {
        Self {
            code,
            message: format!("offered versions {offered:?}; supported versions {supported:?}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NegotiatedVersions {
    pub session_protocol_version: u32,
    pub game_protocol_version: u32,
}

pub fn negotiate_versions(
    offered_session_versions: &[u32],
    offered_game_protocol_versions: &[u32],
) -> Result<NegotiatedVersions, SessionProtocolError> {
    let session_protocol_version = highest_common(
        offered_session_versions,
        SUPPORTED_SESSION_PROTOCOL_VERSIONS,
    )
    .ok_or_else(|| {
        SessionProtocolError::unsupported(
            SessionErrorCode::UnsupportedSessionVersion,
            offered_session_versions,
            SUPPORTED_SESSION_PROTOCOL_VERSIONS,
        )
    })?;
    let game_protocol_version = highest_common(
        offered_game_protocol_versions,
        SUPPORTED_GAME_PROTOCOL_VERSIONS,
    )
    .ok_or_else(|| {
        SessionProtocolError::unsupported(
            SessionErrorCode::UnsupportedGameProtocol,
            offered_game_protocol_versions,
            SUPPORTED_GAME_PROTOCOL_VERSIONS,
        )
    })?;
    Ok(NegotiatedVersions {
        session_protocol_version,
        game_protocol_version,
    })
}

fn highest_common(offered: &[u32], supported: &[u32]) -> Option<u32> {
    offered
        .iter()
        .copied()
        .filter(|version| supported.contains(version))
        .max()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionMessage {
    Hello {
        session_protocol_version: u32,
        client_kind: ClientKind,
        display_name: String,
        supported_session_versions: Vec<u32>,
        supported_game_protocol_versions: Vec<u32>,
    },
    CreateMatch {
        session_protocol_version: u32,
        scenario_id: String,
        controllers: ControllerAssignments,
    },
    JoinMatch {
        session_protocol_version: u32,
        join_token: String,
        display_name: String,
    },
    ParticipantStatus {
        session_protocol_version: u32,
        status: ParticipantStatus,
    },
    Welcome {
        session_protocol_version: u32,
        game_protocol_version: u32,
        connection_id: String,
        can_configure: bool,
        capabilities: Vec<ServerCapability>,
    },
    ScenarioCatalog {
        session_protocol_version: u32,
        scenarios: Vec<ScenarioSummary>,
    },
    LobbyState {
        session_protocol_version: u32,
        state: LobbyPhase,
        scenario: Option<ScenarioSummary>,
        controllers: Option<ControllerAssignments>,
        seats: Vec<SeatLobbyState>,
        bot_policies: Vec<BotPolicySummary>,
        waiting_reason: Option<String>,
    },
    SeatAssigned {
        session_protocol_version: u32,
        side: SideId,
        match_id: String,
        participant_id: String,
    },
    SeatInvitation {
        session_protocol_version: u32,
        side: SideId,
        display_code: String,
        join_token: String,
    },
    Error {
        session_protocol_version: u32,
        code: SessionErrorCode,
        message: String,
    },
}

impl SessionMessage {
    pub fn session_protocol_version(&self) -> u32 {
        match self {
            Self::Hello {
                session_protocol_version,
                ..
            }
            | Self::CreateMatch {
                session_protocol_version,
                ..
            }
            | Self::JoinMatch {
                session_protocol_version,
                ..
            }
            | Self::ParticipantStatus {
                session_protocol_version,
                ..
            }
            | Self::Welcome {
                session_protocol_version,
                ..
            }
            | Self::ScenarioCatalog {
                session_protocol_version,
                ..
            }
            | Self::LobbyState {
                session_protocol_version,
                ..
            }
            | Self::SeatAssigned {
                session_protocol_version,
                ..
            }
            | Self::SeatInvitation {
                session_protocol_version,
                ..
            }
            | Self::Error {
                session_protocol_version,
                ..
            } => *session_protocol_version,
        }
    }

    pub fn validate_version(&self) -> Result<(), SessionProtocolError> {
        let actual = self.session_protocol_version();
        if SUPPORTED_SESSION_PROTOCOL_VERSIONS.contains(&actual) {
            Ok(())
        } else {
            Err(SessionProtocolError::unsupported(
                SessionErrorCode::UnsupportedSessionVersion,
                &[actual],
                SUPPORTED_SESSION_PROTOCOL_VERSIONS,
            ))
        }
    }
}
