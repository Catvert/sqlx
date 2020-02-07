use std::fmt::Display;

use crate::arguments::Arguments;
use crate::connection::Connection;
use crate::cursor::Cursor;
use crate::row::Row;
use crate::types::TypeInfo;

/// A database driver.
///
/// This trait encapsulates a complete driver implementation to a specific
/// database (e.g., MySQL, Postgres).
pub trait Database
where
    Self: Sized + 'static,
    Self: for<'c> HasRawValue<'c>,
    Self: for<'c> HasRow<'c, Database = Self>,
    Self: for<'con> HasCursor<'con, Database = Self>,
{
    /// The concrete `Connection` implementation for this database.
    type Connection: Connection<Database = Self>;

    /// The concrete `Arguments` implementation for this database.
    type Arguments: Arguments<Database = Self>;

    /// The concrete `TypeInfo` implementation for this database.
    type TypeInfo: TypeInfo;

    /// The Rust type of table identifiers for this database.
    type TableId: Display + Clone;

    type RawBuffer;
}

pub trait HasRawValue<'c> {
    type RawValue;
}

pub trait HasRow<'c> {
    type Database: Database;

    type Row: Row<'c, Database = Self::Database>;
}

pub trait HasCursor<'con> {
    type Database: Database;

    type Cursor: Cursor<'con, Database = Self::Database>;
}
