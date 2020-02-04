use crate::database::{Database, HasCursor};

/// A type that contains or can provide a database connection to use for executing queries
/// against the database.
///
/// No guarantees are provided that successive queries run on the same physical database
/// connection. A [`Connection`](trait.Connection.html) is an `Executor` that guarantees that successive
/// queries are run on the same physical database connection.
///
/// Implementations are provided for [`&Pool`](struct.Pool.html),
/// [`&mut PoolConnection`](struct.PoolConnection.html),
/// and [`&mut Connection`](trait.Connection.html).
/// ```
// TODO: Sealed
pub trait Executor<'con>
where
    Self: Send,
{
    /// The specific database that this type is implemented for.
    type Database: Database;

    /// Executes a query that may or may not return a result set.
    fn execute<'q, E>(self, query: E) -> <Self::Database as HasCursor<'con>>::Cursor
    where
        E: Execute<'q, Self::Database>;

    #[doc(hidden)]
    fn execute_by_ref<'q, 'e, E>(
        &'e mut self,
        query: E,
    ) -> <Self::Database as HasCursor<'e>>::Cursor
    where
        E: Execute<'q, Self::Database>;
}

/// A type that may be executed against a database connection.
pub trait Execute<'q, DB>
where
    DB: Database,
{
    /// Returns the query to be executed and the arguments to bind against the query, if any.
    ///
    /// Returning `None` for `Arguments` indicates to use a "simple" query protocol and to not
    /// prepare the query. Returning `Some(Default::default())` is an empty arguments object that
    /// will be prepared (and cached) before execution.
    #[doc(hidden)]
    fn into_parts(self) -> (&'q str, Option<DB::Arguments>);
}

impl<'q, DB> Execute<'q, DB> for &'q str
where
    DB: Database,
{
    #[inline]
    fn into_parts(self) -> (&'q str, Option<DB::Arguments>) {
        (self, None)
    }
}
