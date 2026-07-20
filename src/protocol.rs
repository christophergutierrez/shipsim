/// External NDJSON protocol version.
///
/// Protocol 4 (simplified simultaneous turns, ADR-0025):
/// - three collection stages: allocate → path → volley;
/// - one complete path and one complete volley per living ship per turn;
/// - no velocity/course, no four-cycle impulses, no ready_fire, no end_turn;
/// - automatic turn advance after volley resolution;
/// - weapon charge carries; shields and motion do not.
///
/// Protocol-v3 (and older) saves and clients are rejected by version checks.
pub const PROTOCOL_VERSION: u32 = 4;
