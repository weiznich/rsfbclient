//!
//! Rust Firebird Client
//!

#[cfg(test)]
#[macro_use]
pub(crate) mod tests;

pub mod prelude {
    pub use crate::query::{Execute, Queryable};
}

mod connection;
mod query;
mod statement;
mod transaction;

pub use crate::{
    connection::{Connection, ConnectionConfiguration, FirebirdClientFactory},
    query::{Execute, Queryable},
    statement::Statement,
    transaction::Transaction,
};
pub use rsfbclient_core::{
    charset, Charset, Column, Dialect, FbError, FromRow, IntoParam, Row, SqlType,
};

#[cfg(feature = "pool")]
pub use crate::connection::pool::FirebirdConnectionManager;

//builders are behind feature gates inside this module
pub use crate::connection::builders;
pub use builders::*;
