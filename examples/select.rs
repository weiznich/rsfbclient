//!
//! Rust Firebird Client
//!
//! Example of select
//!
//! You need create a database with this table:
//! create table test (col_a int generated by default as identity, col_b float, col_c varchar(10));
//!
//! You can use the insert example to populate
//! the database ;)
//!

#![allow(unused_variables, unused_mut)]

use rsfbclient::{prelude::*, FbError};

fn main() -> Result<(), FbError> {
    #[cfg(feature = "linking")]
    let mut conn = rsfbclient::builder_native()
        .with_dyn_link()
        .with_remote()
        .host("localhost")
        .db_name("examples.fdb")
        .user("SYSDBA")
        .pass("masterkey")
        .connect()?;

    #[cfg(feature = "dynamic_loading")]
    let mut conn = rsfbclient::builder_native()
        .with_dyn_load("./fbclient.lib")
        .with_remote()
        .host("localhost")
        .db_name("examples.fdb")
        .user("SYSDBA")
        .pass("masterkey")
        .connect()?;

    #[cfg(feature = "pure_rust")]
    let mut conn = rsfbclient::builder_pure_rust()
        .host("localhost")
        .db_name("examples.fdb")
        .user("SYSDBA")
        .pass("masterkey")
        .connect()?;

    // `query_iter` for large quantities of rows, will allocate space for one row at a time
    let rows = conn.query_iter("select col_a, col_b, col_c from test", ())?;

    println!("| col_a | col_b | col_c   |");
    println!("| ----- | ----- | ------- |");
    for row in rows {
        let (col_a, col_b, col_c): (i32, f32, String) = row?;

        println!("| {:^5} | {:^5} | {:7} |", col_a, col_b, col_c);
    }

    // `query` for small quantities of rows, will allocate a vector with all rows
    let rows: Vec<(i32, f32, String)> = conn.query("select col_a, col_b, col_c from test", ())?;

    println!("{:?}", rows);

    Ok(())
}
