/// External NDJSON protocol version.
///
/// M6 (ADR-0022) increments the protocol from v1 to v2: order and snapshot
/// contracts change to maneuver commitment semantics, and snapshots expose
/// velocity/course/facing/thrust/movement-phase/commitments. Protocol-v1
/// saves are rejected by version at `SaveDocument::read` before replay or
/// order-shape interpretation. Only v2 is emitted or accepted externally.
pub const PROTOCOL_VERSION: u32 = 2;
