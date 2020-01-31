use crate::database::{Database, HasCursor, HasRawRow};
use crate::describe::Describe;
use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use futures_util::TryStreamExt;

/// Encapsulates query execution on the database.
///
/// Implemented primarily by [crate::Pool].
pub trait Executor {
    type Database: Database + ?Sized;

    /// Send a raw SQL command to the database.
    ///
    /// This is intended for queries that cannot or should not be prepared (ex. `BEGIN`).
    ///
    /// Does not support fetching results.
    fn send<'e, 'q: 'e>(&'e mut self, command: &'q str) -> BoxFuture<'e, crate::Result<()>>;

    /// Execute the query, returning the number of rows affected.
    fn execute<'e, 'q: 'e>(
        &'e mut self,
        query: &'q str,
        args: <Self::Database as Database>::Arguments,
    ) -> BoxFuture<'e, crate::Result<u64>>;

    /// Executes the query and returns a [Stream] of [Row].
    fn fetch<'e, 'q: 'e>(
        &'e mut self,
        query: &'q str,
        args: <Self::Database as Database>::Arguments,
    ) -> <Self::Database as HasCursor<'e>>::Cursor;

    // /// Executes the query and returns up to resulting record.
    // ///
    // /// * [crate::Error::FoundMoreThanOne] will be returned if the query produced more than 1 row.
    // fn fetch_optional<'e, 'q: 'e>(
    //     &'e mut self,
    //     query: &'q str,
    //     args: <Self::Database as Database>::Arguments,
    // ) -> BoxFuture<'e, crate::Result<Option<<Self::Database as HasRawRow<'e>>::RawRow>>> {
    //     let mut s = self.fetch(query, args);
    //     Box::pin(async move {
    //         match s.try_next().await? {
    //             Some(val) => {
    //                 if s.try_next().await?.is_some() {
    //                     Err(crate::Error::FoundMoreThanOne)
    //                 } else {
    //                     Ok(Some(val))
    //                 }
    //             }
    //             None => Ok(None),
    //         }
    //     })
    // }

    // /// Execute the query and return at most one resulting record.
    // fn fetch_one<'e, 'q: 'e>(
    //     &'e mut self,
    //     query: &'q str,
    //     args: <Self::Database as Database>::Arguments,
    // ) -> BoxFuture<'e, crate::Result<<Self::Database as HasRawRow<'e>>::RawRow>> {
    //     let mut s = self.fetch(query, args);
    //     Box::pin(async move { s.try_next().await?.ok_or(crate::Error::NotFound) })
    // }

    #[doc(hidden)]
    fn describe<'e, 'q: 'e>(
        &'e mut self,
        query: &'q str,
    ) -> BoxFuture<'e, crate::Result<Describe<Self::Database>>>;
}
