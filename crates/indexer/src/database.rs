// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

//! Database-related functions
#![allow(clippy::extra_unused_lifetimes)]
use crate::util::remove_null_bytes;
use diesel::{
    pg::PgConnection,
    r2d2::{ConnectionManager, PoolError, PooledConnection},
    RunQueryDsl,
};
use std::{cmp::min, sync::Arc};

pub type PgPool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;
pub type PgDbPool = Arc<PgPool>;
pub type PgPoolConnection = PooledConnection<ConnectionManager<PgConnection>>;

pub const MAX_DIESEL_PARAM_SIZE: u16 = u16::MAX;

/// Given diesel has a limit of how many parameters can be inserted in a single operation (u16::MAX)
/// we may need to chunk an array of items based on how many columns are in the table.
/// This function returns boundaries of chunks in the form of (start_index, end_index)
pub fn get_chunks(num_items_to_insert: usize, column_count: usize) -> Vec<(usize, usize)> {
    let max_item_size = MAX_DIESEL_PARAM_SIZE as usize / column_count;
    let mut chunk: (usize, usize) = (0, min(num_items_to_insert, max_item_size));
    let mut chunks = vec![chunk];
    while chunk.1 != num_items_to_insert {
        chunk = (
            chunk.0 + max_item_size,
            min(num_items_to_insert, chunk.1 + max_item_size),
        );
        chunks.push(chunk);
    }
    chunks
}

/// This function will clean the data for postgres. Currently it has support for removing
/// null bytes from strings but in the future we will add more functionality.
pub fn clean_data_for_db<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(
    items: Vec<T>,
    should_remove_null_bytes: bool,
) -> Vec<T> {
    if should_remove_null_bytes {
        items.iter().map(remove_null_bytes).collect()
    } else {
        items
    }
}

pub fn new_db_pool(database_url: &str) -> Result<PgDbPool, PoolError> {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    PgPool::builder().build(manager).map(Arc::new)
}

pub fn execute_with_better_error<
    T: diesel::Table + diesel::QuerySource,
    U: diesel::query_builder::QueryFragment<diesel::pg::Pg>
        + diesel::insertable::CanInsertInSingleQuery<diesel::pg::Pg>,
>(
    conn: &PgPoolConnection,
    query: diesel::query_builder::InsertStatement<T, U>,
) -> diesel::QueryResult<usize>
where
    <T as diesel::QuerySource>::FromClause: diesel::query_builder::QueryFragment<diesel::pg::Pg>,
{
    let debug = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();
    aptos_logger::debug!("Executing query: {:?}", debug);
    let res = query.execute(conn);
    if let Err(ref e) = res {
        aptos_logger::warn!("Error running query: {:?}\n{}", e, debug);
    }
    res
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_get_chunks_logic() {
        assert_eq!(get_chunks(10, 5), vec![(0, 10)]);
        assert_eq!(get_chunks(65535, 1), vec![(0, 65535)]);
        // 200,000 total items will take 6 buckets. Each bucket can only be 3276 size.
        assert_eq!(
            get_chunks(10000, 20),
            vec![(0, 3276), (3276, 6552), (6552, 9828), (9828, 10000)]
        );
        assert_eq!(
            get_chunks(65535, 2),
            vec![(0, 32767), (32767, 65534), (65534, 65535)]
        );
        assert_eq!(
            get_chunks(65535, 3),
            vec![(0, 21845), (21845, 43690), (43690, 65535)]
        );
    }
}
