//! Wire bounds and diagnostic resource budgets. Keep protocol bounds aligned
//! with WIRE.md; diagnostic budgets may be stricter than the synchronization SDK.
pub(super) const MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub(super) const FIELDS: usize = 500_000;
pub(super) const NESTING: usize = 128;
pub(super) const METADATA: usize = 4;
pub(super) const ACTIONS: usize = 65_535;
pub(super) const STRING_PATH_SLOTS: usize = 4_096;
pub(super) const BINARY_SLOTS: usize = 256;
pub(super) const BINARY_BYTES: usize = 65_536;
pub(super) const PATCH_BYTES: usize = 1024 * 1024;
pub(super) const PATH_SEGMENTS: usize = 256;
pub(super) const PATH_KEY_BYTES: usize = 64 * 1024;
pub(super) const AOO_ROWS: usize = 65_536;
pub(super) const AOO_COLUMNS: usize = 4_096;
pub(super) const AOO_CELLS: usize = 1_048_576;
pub(super) const AOO_EMPTY_ROWS: usize = 256;
pub(super) const AOO_ENTRIES: usize = 262_144;
