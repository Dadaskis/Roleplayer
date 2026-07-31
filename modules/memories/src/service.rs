//! Memory orchestration: CRUD + best-effort summarization via the provider.

use std::sync::Arc;

use roleplayer_core::errors::Result;
use roleplayer_core::llm::{
    ChatMessage, CompletionRequest, ContentBlock, Role,
};
use roleplayer_core::storage::Storage;
use roleplayer_core::{new_id, now_rfc3339};
use roleplayer_providers::registry::ProviderRegistry;

use crate::domain::{Memory, NewMemory};
use crate::storage as repo;

/// Orchestrates memories: manual CRUD and provider-backed summarization.
pub struct MemoryService<S: Storage> {
    storage: Arc<S>,
    providers: Arc<ProviderRegistry>,
}

impl<S: Storage> MemoryService<S> {
    /// Create the service over the storage seam and the provider registry.
    pub fn new(
        storage: Arc<S>,
        providers: Arc<ProviderRegistry>,
    ) -> MemoryService<S> {
        MemoryService { storage, providers }
    }

    /// Memories of a campaign, newest first.
    pub fn list_for_campaign(&self, campaign_id: &str) -> Result<Vec<Memory>> {
        repo::list_for_campaign(self.storage.as_ref(), campaign_id)
    }

    /// Create a memory (manual / GM-curated).
    pub fn create(&self, input: NewMemory) -> Result<Memory> {
        input.validate()?;
        let memory = Memory {
            id: new_id(),
            campaign_id: input.campaign_id.trim().to_string(),
            summary: input.summary.trim().to_string(),
            source_from: input.source_from,
            source_to: input.source_to,
            created_at: now_rfc3339(),
        };
        repo::insert(self.storage.as_ref(), &memory)?;
        tracing::info!(campaign_id = %memory.campaign_id, "memory created");
        Ok(memory)
    }

    /// Delete a memory; `true` if one was deleted.
    pub fn delete(&self, memory_id: &str) -> Result<bool> {
        let deleted = repo::delete(self.storage.as_ref(), memory_id)?;
        tracing::info!(memory_id, deleted, "memory delete requested");
        Ok(deleted)
    }

    /// Summarize a turn range with the default provider and store the result.
    ///
    /// Best-effort: a provider outage is a typed error the UI surfaces — never
    /// a panic, never a partial write (the memory is inserted only on success).
    pub async fn generate_summary(
        &self,
        campaign_id: &str,
        source_from: i64,
        source_to: i64,
    ) -> Result<Memory> {
        let messages = roleplayer_turnflow::storage::list_messages_between(
            self.storage.as_ref(),
            campaign_id,
            source_from,
            source_to,
        )?;
        if messages.is_empty() {
            return Err(roleplayer_core::errors::AppError::Domain(
                "no messages in the given turn range".to_string(),
            ));
        }

        let transcript = messages
            .iter()
            .map(|message| {
                let text = message
                    .content
                    .iter()
                    .filter_map(ContentBlock::text)
                    .collect::<Vec<&str>>()
                    .join(" ");
                format!("[{}] {}", message.role.as_str(), text)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let provider = self.providers.require_default()?;
        let model = self.providers.default_model().ok_or_else(|| {
            roleplayer_core::errors::AppError::Config(
                "no default model configured".to_string(),
            )
        })?;
        let request = CompletionRequest {
            model,
            messages: vec![
                ChatMessage::text(
                    Role::System,
                    "You are a game chronicler. Summarize the roleplay transcript \
                     into 2-4 concise sentences covering only facts a future GM \
                     must remember: locations, items, NPCs, injuries, world \
                     changes. Do not invent anything not in the transcript.",
                ),
                ChatMessage::text(Role::User, transcript),
            ],
            tools: vec![],
            temperature: Some(0.2),
            max_tokens: Some(500),
            stream: false,
        };

        let response = provider.complete(request).await?;
        let summary = response
            .message
            .content
            .iter()
            .filter_map(ContentBlock::text)
            .collect::<Vec<&str>>()
            .join(" ")
            .trim()
            .to_string();

        if summary.is_empty() {
            return Err(roleplayer_core::errors::AppError::Provider(
                "provider returned an empty summary".to_string(),
            ));
        }

        let memory = Memory {
            id: new_id(),
            campaign_id: campaign_id.to_string(),
            summary,
            source_from,
            source_to,
            created_at: now_rfc3339(),
        };
        repo::insert(self.storage.as_ref(), &memory)?;
        tracing::info!(campaign_id, source_from, source_to, "memory generated");
        Ok(memory)
    }
}
