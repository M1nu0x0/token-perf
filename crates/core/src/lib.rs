pub mod common;
#[cfg(feature = "serve")]
pub mod server;
pub mod sources;
pub mod store;

pub use common::model::Session;

use store::{ScanReport, Store};

/// The store comes back so the caller can read and write settings on the same
/// connection.
pub fn sync_and_load(rescan: bool) -> rusqlite::Result<(Store, Vec<Session>, ScanReport)> {
    let mut store = Store::open_default()?;
    if rescan {
        store.clear_cursors()?;
    }
    let report = store.sync()?;
    let sessions = store.sessions()?;
    Ok((store, sessions, report))
}
