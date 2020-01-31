//! Traits for passing arguments to SQL queries.

use crate::database::Database;
use crate::encode::Encode;
use crate::types::Type;

/// A tuple of arguments to be sent to the database.
pub trait Arguments: Send + Sized + Default + 'static {
    type Database: Database + ?Sized;

    /// Returns `true` if there are no values.
    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the number of values.
    fn len(&self) -> usize;

    #[deprecated]
    fn size(&self) -> usize {
        0
    }

    /// Reserves the capacity for at least `len` more values (of `size` bytes) to
    /// be added to the arguments without a reallocation.  
    fn reserve(&mut self, len: usize, size_hint: usize);

    /// Add the value to the end of the arguments.
    fn add<T>(&mut self, value: T)
    where
        T: Type<Self::Database>,
        T: Encode<Self::Database>;
}

pub trait IntoArguments<DB>
where
    DB: Database,
{
    fn into_arguments(self) -> DB::Arguments;
}

impl<A> IntoArguments<A::Database> for A
where
    A: Arguments,
    A::Database: Database<Arguments = Self> + Sized,
{
    #[inline]
    fn into_arguments(self) -> Self {
        self
    }
}

#[doc(hidden)]
pub struct ImmutableArguments<DB: Database>(pub DB::Arguments);

impl<DB: Database> IntoArguments<DB> for ImmutableArguments<DB> {
    fn into_arguments(self) -> <DB as Database>::Arguments {
        self.0
    }
}
