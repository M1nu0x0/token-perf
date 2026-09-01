pub mod common;
pub mod server;
pub mod sources;
pub mod store;

pub use common::model::Session;

use store::{ScanReport, Store};

pub fn sync_and_load() -> rusqlite::Result<(Vec<Session>, ScanReport)> {
    let mut store = Store::open_default()?;
    let report = store.sync()?;
    Ok((store.sessions()?, report))
}
