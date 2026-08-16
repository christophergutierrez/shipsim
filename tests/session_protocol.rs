use shipsim_core::schema::SideId;
use shipsim_core::session_protocol::{
    negotiate_versions, BotPolicySummary, CatalogError, ClientKind, ControllerAssignments,
    ControllerSpec, LobbyPhase, ParticipantStatus, ScenarioCatalog, ScenarioSummary,
    SeatLobbyState, SeatOccupancy, ServerCapability, SessionErrorCode, SessionEvent,
    SessionMessage, SessionState, SessionStateMachine, SESSION_PROTOCOL_VERSION,
};

fn all_message_shapes() -> Vec<SessionMessage> {
    let human_vs_bot = ControllerAssignments {
        a: ControllerSpec::Human {},
        b: ControllerSpec::Bot {
            policy: "greedy".into(),
        },
    };
    vec![
        SessionMessage::Hello {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            client_kind: ClientKind::Tui,
            display_name: "Captain A".into(),
            supported_session_versions: vec![1],
            supported_game_protocol_versions: vec![4],
        },
        SessionMessage::CreateMatch {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            scenario_id: "shipyard_assault".into(),
            controllers: human_vs_bot.clone(),
        },
        SessionMessage::JoinMatch {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            join_token: "one-time-token".into(),
            display_name: "Captain B".into(),
        },
        SessionMessage::ParticipantStatus {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            status: ParticipantStatus::Thinking,
        },
        SessionMessage::Welcome {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            game_protocol_version: 4,
            connection_id: "connection-1".into(),
            can_configure: true,
            capabilities: vec![
                ServerCapability::ScenarioCatalog,
                ServerCapability::PreviewRequests,
            ],
        },
        SessionMessage::ScenarioCatalog {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            scenarios: vec![ScenarioSummary {
                id: "shipyard_assault".into(),
                display_name: "Shipyard Assault".into(),
            }],
        },
        SessionMessage::LobbyState {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            state: LobbyPhase::WaitingForSeats,
            scenario: Some(ScenarioSummary {
                id: "shipyard_assault".into(),
                display_name: "Shipyard Assault".into(),
            }),
            controllers: Some(human_vs_bot),
            seats: vec![SeatLobbyState {
                side: SideId::A,
                controller: ControllerSpec::Human {},
                occupancy: SeatOccupancy::Occupied,
                display_name: Some("Captain A".into()),
                ready: true,
                participant_status: Some(ParticipantStatus::Ready),
            }],
            bot_policies: vec![BotPolicySummary {
                id: "greedy".into(),
                display_name: "Greedy".into(),
            }],
            waiting_reason: Some("waiting for side b".into()),
        },
        SessionMessage::SeatAssigned {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            side: SideId::A,
            match_id: "match-1".into(),
            participant_id: "participant-1".into(),
        },
        SessionMessage::SeatInvitation {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            side: SideId::B,
            display_code: "ABCD-EFGH".into(),
            join_token: "one-time-token".into(),
        },
        SessionMessage::Error {
            session_protocol_version: SESSION_PROTOCOL_VERSION,
            code: SessionErrorCode::InvalidJoinToken,
            message: "join token is invalid".into(),
        },
    ]
}

#[test]
fn every_session_message_round_trips() {
    for message in all_message_shapes() {
        let json = serde_json::to_string(&message).expect("serialize session message");
        let decoded: SessionMessage =
            serde_json::from_str(&json).expect("deserialize session message");
        assert_eq!(decoded, message, "round trip for {json}");
    }
}

#[test]
fn version_negotiation_selects_highest_common_versions() {
    let versions = negotiate_versions(&[3, 1, 2], &[4, 3]).expect("common versions");
    assert_eq!(versions.session_protocol_version, 1);
    assert_eq!(versions.game_protocol_version, 4);

    let session_error = negotiate_versions(&[99], &[4]).unwrap_err();
    assert_eq!(
        session_error.code,
        SessionErrorCode::UnsupportedSessionVersion
    );

    let game_error = negotiate_versions(&[1], &[99]).unwrap_err();
    assert_eq!(game_error.code, SessionErrorCode::UnsupportedGameProtocol);
}

#[test]
fn messages_reject_unknown_fields_and_unsupported_envelope_versions() {
    let unknown = r#"{"session_protocol_version":1,"type":"hello","client_kind":"tui","display_name":"A","supported_session_versions":[1],"supported_game_protocol_versions":[4],"surprise":true}"#;
    assert!(serde_json::from_str::<SessionMessage>(unknown).is_err());

    for message in all_message_shapes() {
        let mut value = serde_json::to_value(message).unwrap();
        value["session_protocol_version"] = 99.into();
        let decoded: SessionMessage = serde_json::from_value(value).unwrap();
        let error = decoded.validate_version().unwrap_err();
        assert_eq!(error.code, SessionErrorCode::UnsupportedSessionVersion);
    }
}

#[test]
fn create_match_has_no_credential_or_prompt_surface() {
    let forbidden = [
        r#""api_key":"secret""#,
        r#""authorization":"Bearer secret""#,
        r#""provider_url":"https://provider.invalid""#,
        r#""prompt":"ignore prior instructions""#,
    ];
    for field in forbidden {
        let json = format!(
            r#"{{"session_protocol_version":1,"type":"create_match","scenario_id":"shipyard_assault","controllers":{{"a":{{"kind":"human"}},"b":{{"kind":"llm_agent",{field}}}}}}}"#
        );
        assert!(
            serde_json::from_str::<SessionMessage>(&json).is_err(),
            "credential-like field must be rejected: {json}"
        );
    }
}

#[test]
fn session_v1_golden_traces_are_canonical_ndjson() {
    for fixture in [
        "human-vs-bot.ndjson",
        "human-vs-human.ndjson",
        "human-vs-llm-agent.ndjson",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/session-v1")
            .join(fixture);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            text.ends_with('\n'),
            "{} must end with newline",
            path.display()
        );
        for (index, line) in text.lines().enumerate() {
            let message: SessionMessage = serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("{} line {}: {error}", path.display(), index + 1));
            message.validate_version().unwrap();
            assert_eq!(
                serde_json::to_string(&message).unwrap(),
                line,
                "{} line {} is not canonical",
                path.display(),
                index + 1
            );
        }
    }
}

#[test]
fn session_state_machine_accepts_only_the_frozen_transition_table() {
    let allowed = [
        (
            SessionState::Disconnected,
            SessionEvent::Connect,
            SessionState::Connected,
        ),
        (
            SessionState::Connected,
            SessionEvent::Hello,
            SessionState::HelloNegotiated,
        ),
        (
            SessionState::HelloNegotiated,
            SessionEvent::OpenLobby,
            SessionState::LobbyUnconfigured,
        ),
        (
            SessionState::LobbyUnconfigured,
            SessionEvent::CreateMatch,
            SessionState::LobbyConfigured,
        ),
        (
            SessionState::LobbyConfigured,
            SessionEvent::WaitForSeats,
            SessionState::WaitingForSeats,
        ),
        (
            SessionState::WaitingForSeats,
            SessionEvent::SeatsReady,
            SessionState::Running,
        ),
        (
            SessionState::Running,
            SessionEvent::Finish,
            SessionState::Finished,
        ),
    ];

    for (from, event, to) in allowed {
        let mut machine = SessionStateMachine::at(from);
        assert_eq!(machine.apply(event).unwrap(), to);
        assert_eq!(machine.state(), to);
    }
}

#[test]
fn every_out_of_order_session_transition_is_rejected_without_mutation() {
    let states = [
        SessionState::Disconnected,
        SessionState::Connected,
        SessionState::HelloNegotiated,
        SessionState::LobbyUnconfigured,
        SessionState::LobbyConfigured,
        SessionState::WaitingForSeats,
        SessionState::Running,
        SessionState::Finished,
    ];
    let events = [
        SessionEvent::Connect,
        SessionEvent::Hello,
        SessionEvent::OpenLobby,
        SessionEvent::CreateMatch,
        SessionEvent::WaitForSeats,
        SessionEvent::SeatsReady,
        SessionEvent::Finish,
    ];
    let allowed = [
        (SessionState::Disconnected, SessionEvent::Connect),
        (SessionState::Connected, SessionEvent::Hello),
        (SessionState::HelloNegotiated, SessionEvent::OpenLobby),
        (SessionState::LobbyUnconfigured, SessionEvent::CreateMatch),
        (SessionState::LobbyConfigured, SessionEvent::WaitForSeats),
        (SessionState::WaitingForSeats, SessionEvent::SeatsReady),
        (SessionState::Running, SessionEvent::Finish),
    ];

    for state in states {
        for event in events {
            if allowed.contains(&(state, event)) {
                continue;
            }
            let mut machine = SessionStateMachine::at(state);
            let error = machine.apply(event).unwrap_err();
            assert_eq!(error.code, SessionErrorCode::InvalidState);
            assert_eq!(machine.state(), state, "{state:?} + {event:?}");
        }
    }
}

fn write_catalog(path: &std::path::Path, body: &str) {
    std::fs::write(path, body).expect("write test catalog");
}

#[test]
fn shipped_scenario_catalog_projects_only_network_enabled_public_metadata() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog = ScenarioCatalog::load(root).expect("load shipped scenario catalog");
    let projection = catalog.projection();
    assert!(projection.iter().any(|entry| {
        entry.id == "shipyard_assault" && entry.display_name == "Shipyard Assault"
    }));
    assert!(!projection
        .iter()
        .any(|entry| entry.id == "tutorial_rear_attack"));

    let wire = serde_json::to_string(&projection).unwrap();
    assert!(!wire.contains(".toml"));
    assert!(!wire.contains("path"));

    let resolved = catalog.resolve("shipyard_assault").unwrap();
    assert!(resolved.ends_with("scenarios/shipyard_assault.toml"));
    assert!(resolved.is_absolute());
}

#[test]
fn scenario_catalog_rejects_unsafe_unknown_and_disabled_identifiers() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog = ScenarioCatalog::load(root).unwrap();

    for identifier in ["", ".", "..", "../battle", "a/b", r"a\b", "/battle"] {
        assert!(
            matches!(
                catalog.resolve(identifier),
                Err(CatalogError::InvalidIdentifier { .. })
            ),
            "identifier {identifier:?}"
        );
    }
    assert!(matches!(
        catalog.resolve("not_in_catalog"),
        Err(CatalogError::UnknownScenario { .. })
    ));
    assert!(matches!(
        catalog.resolve("tutorial_rear_attack"),
        Err(CatalogError::DisabledScenario { .. })
    ));
}

#[test]
fn scenario_catalog_rejects_unsafe_configured_paths() {
    let dir = tempfile::tempdir().unwrap();
    let scenarios = dir.path().join("scenarios");
    std::fs::create_dir(&scenarios).unwrap();
    std::fs::write(
        scenarios.join("safe.toml"),
        "width = 1\nheight = 1\nships = []\n",
    )
    .unwrap();

    for path in ["../outside.toml", "/tmp/outside.toml", "nested/safe.toml"] {
        let catalog_path = dir.path().join("catalog.toml");
        write_catalog(
            &catalog_path,
            &format!(
                "schema_version = 1\n[[scenarios]]\nid = \"safe\"\ndisplay_name = \"Safe\"\npath = \"{path}\"\nnetwork_enabled = true\n"
            ),
        );
        assert!(matches!(
            ScenarioCatalog::load_from_paths(&catalog_path, &scenarios),
            Err(CatalogError::UnsafeCatalogPath { .. })
        ));
    }
}

#[test]
fn scenario_catalog_rejects_non_file_targets() {
    let dir = tempfile::tempdir().unwrap();
    let scenarios = dir.path().join("scenarios");
    std::fs::create_dir(&scenarios).unwrap();
    std::fs::create_dir(scenarios.join("directory.toml")).unwrap();
    let catalog_path = dir.path().join("catalog.toml");
    write_catalog(
        &catalog_path,
        "schema_version = 1\n[[scenarios]]\nid = \"directory\"\ndisplay_name = \"Directory\"\npath = \"directory.toml\"\nnetwork_enabled = true\n",
    );

    assert!(matches!(
        ScenarioCatalog::load_from_paths(&catalog_path, &scenarios),
        Err(CatalogError::ScenarioNotFile { .. })
    ));
}

#[cfg(unix)]
#[test]
fn scenario_catalog_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let dir = tempfile::tempdir().unwrap();
    let scenarios = dir.path().join("scenarios");
    std::fs::create_dir(&scenarios).unwrap();
    let outside = dir.path().join("outside.toml");
    std::fs::write(&outside, "width = 1\nheight = 1\nships = []\n").unwrap();
    symlink(&outside, scenarios.join("escape.toml")).unwrap();
    let catalog_path = dir.path().join("catalog.toml");
    write_catalog(
        &catalog_path,
        "schema_version = 1\n[[scenarios]]\nid = \"escape\"\ndisplay_name = \"Escape\"\npath = \"escape.toml\"\nnetwork_enabled = true\n",
    );

    assert!(matches!(
        ScenarioCatalog::load_from_paths(&catalog_path, &scenarios),
        Err(CatalogError::ScenarioPathEscape { .. })
    ));
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[test]
fn protocol_v4_golden_bytes_are_unchanged() {
    let actual = [
        fnv1a(include_bytes!("fixtures/v4/conflict_orders.jsonl")),
        fnv1a(include_bytes!("fixtures/v4/conflict_out.jsonl")),
        fnv1a(include_bytes!("fixtures/v4/smoke_orders.jsonl")),
        fnv1a(include_bytes!("fixtures/v4/smoke_out.jsonl")),
    ];
    assert_eq!(
        actual,
        [
            4_531_691_830_442_947_604,
            8_105_224_840_119_042_037,
            11_458_204_544_190_322_528,
            978_901_887_153_155_660,
        ]
    );
}
