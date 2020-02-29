use std::ops::DerefMut;

use futures_core::{future::BoxFuture, stream::BoxStream};
use futures_util::StreamExt;

use crate::{
    connection::{Connect, Connection},
    describe::Describe,
    executor::Executor,
    pool::Pool,
    Cursor, Database, Error,
};

use super::PoolConnection;
use crate::database::HasCursor;
use crate::executor::Execute;
use tokio::macros::support::Future;

impl<'p, C, DB> Executor<'p> for &'p Pool<C>
where
    C: Connect<Database = DB>,
    DB: Database<Connection = C>,
    DB: for<'c, 'q> HasCursor<'c, 'q, DB>,
    for<'con> &'con mut C: Executor<'con>,
{
    type Database = DB;

    fn fetch<'q, E>(self, query: E) -> <Self::Database as HasCursor<'p, 'q, DB>>::Cursor
    where
        E: Execute<'q, DB>,
    {
        DB::Cursor::from_pool(self, query)
    }

    #[doc(hidden)]
    #[inline]
    fn fetch_by_ref<'q, 'e, E>(
        &'e mut self,
        query: E,
    ) -> <Self::Database as HasCursor<'_, 'q, DB>>::Cursor
    where
        E: Execute<'q, DB>,
    {
        self.fetch(query)
    }
}

impl<'c, C, DB> Executor<'c> for &'c mut PoolConnection<C>
where
    C: Connect<Database = DB>,
    DB: Database<Connection = C>,
    DB: for<'c2, 'q> HasCursor<'c2, 'q, DB>,
    for<'con> &'con mut C: Executor<'con>,
{
    type Database = C::Database;

    fn execute<'q, E>(&mut self, query: E) -> BoxFuture<'_, Result<u64, Error>>
    where
        E: Execute<'q, Self::Database>,
    {
        (**self).execute(query)
    }

    fn fetch<'q, E>(self, query: E) -> <Self::Database as HasCursor<'c, 'q, DB>>::Cursor
    where
        E: Execute<'q, Self::Database>,
    {
        (**self).fetch(query)
    }

    #[doc(hidden)]
    #[inline]
    fn fetch_by_ref<'q, 'e, E>(
        &'e mut self,
        query: E,
    ) -> <Self::Database as HasCursor<'_, 'q, DB>>::Cursor
    where
        E: Execute<'q, Self::Database>,
    {
        self.fetch(query)
    }
}

impl<C, DB> Executor<'static> for PoolConnection<C>
where
    C: Connect<Database = DB>,
    DB: Database<Connection = C>,
    DB: for<'c, 'q> HasCursor<'c, 'q, DB>,
{
    type Database = DB;

    fn fetch<'q, E>(self, query: E) -> <DB as HasCursor<'static, 'q, DB>>::Cursor
    where
        E: Execute<'q, Self::Database>,
    {
        DB::Cursor::from_connection(self, query)
    }

    #[doc(hidden)]
    #[inline]
    fn fetch_by_ref<'q, 'e, E>(&'e mut self, query: E) -> <DB as HasCursor<'_, 'q, DB>>::Cursor
    where
        E: Execute<'q, Self::Database>,
    {
        DB::Cursor::from_connection(&mut **self, query)
    }
}
