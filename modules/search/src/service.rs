//! Search orchestration: exposes transcript search over the storage seam.

use std::sync::Arc;

use roleplayer_core::errors::Result;
use roleplayer_core::storage::Storage;

use crate::domain::SearchResult;
use crate::storage as repo;

/// Orchestrates full-text search over transcripts.
pub struct SearchService<S: Storage> {
    storage: Arc<S>,
}

impl<S: Storage> SearchService<S> {
    /// Create the service over the shared storage seam.
    pub fn new(storage: Arc<S>) -> SearchService<S> {
        SearchService { storage }
    }

    /// Search a campaign's transcript for `query`, newest-first.
    pub fn search(
        &self,
        campaign_id: &str,
        query: &str,
        limit: i64,
    ) -> Result<Vec<SearchResult>> {
        let limit = limit.clamp(1, 200);
        let results =
            repo::search(self.storage.as_ref(), campaign_id, query, limit)?;
        tracing::debug!(
            campaign_id,
            query,
            hits = results.len(),
            "search executed"
        );
        Ok(results)
    }
}
