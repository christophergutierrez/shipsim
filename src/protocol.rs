/// External NDJSON protocol version.
///
/// Protocol 3 (combat model refresh):
/// - weapon charge carries across turns (allocate only pays for increases; cannot strip);
/// - shields always re-bought from 0 each allocate;
/// - maneuvers are coast / accel / turn{facing}; max velocity 8;
/// - each movement phase slides `speed` hexes along course (constant rate).
///
/// Protocol-v1/v2 saves and clients are rejected by version checks.
pub const PROTOCOL_VERSION: u32 = 3;
