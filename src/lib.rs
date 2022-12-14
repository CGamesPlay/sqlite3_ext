#![cfg_attr(docsrs, feature(doc_cfg))]
pub use connection::*;
pub use extension::Extension;
pub use globals::*;
pub use iterator::*;
pub use sqlite3_ext_macro::*;
pub use transaction::*;
pub use types::*;
pub use value::*;

mod connection;
mod extension;
pub mod ffi;
pub mod function;
mod globals;
mod iterator;
mod mutex;
pub mod query;
mod test_helpers;
mod transaction;
mod types;
mod value;
pub mod vtab;
mod with_rusqlite;

/// Indicate the risk level for a function or virtual table.
///
/// It is recommended that all functions and virtual table implementations set a risk level,
/// but the default is [RiskLevel::Innocuous] if TRUSTED_SCHEMA=on and [RiskLevel::DirectOnly]
/// otherwise.
///
/// See [this discussion](https://www.sqlite.org/src/doc/latest/doc/trusted-schema.md) for more
/// details about the motivation and implications.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum RiskLevel {
    /// An innocuous function or virtual table is one that can only read content from the
    /// database file in which it resides, and can only alter the database in which it
    /// resides.
    Innocuous,
    /// A direct-only function or virtual table has side-effects that go outside the
    /// database file in which it lives, or return information from outside of the database
    /// file.
    DirectOnly,
}
