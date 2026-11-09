use crate::config::{ModelConfig, ModelsConfig};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider is not configured")]
    NotConfigured,
    #[error("provider request failed: {0}")]
    Request(String),
    #[error("provider returned an invalid response: {0}")]
    InvalidResponse(String),
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn generate(&self, system: &str, user: &str) -> Result<String, ProviderError>;
    fn id(&self) -> String;
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError>;
    fn id(&self) -> String;
}

#[async_trait]
pub trait JudgeProvider: Send + Sync {
    /// Return one score per candidate, in the same order. The caller keeps
    /// membership unchanged and only reorders by these scores.
    async fn rerank(&self, query: &str, candidates: &[String])
        -> Result<Vec<f32>, ProviderError>;
    fn id(&self) -> String;
}

pub struct ExtractionInput<'a> {
    pub chunk_id: &'a str,
    pub doc_id: &'a str,
    pub text: &'a str,
}

#[async_trait]
pub trait EventExtractor: Send + Sync {
    async fn extract(&self, input: &ExtractionInput<'_>)
        -> Result<crate::event::ExtractedEvent, ProviderError>;
    fn version(&self) -> String;
    fn model_id(&self) -> String;
}

#[derive(Clone)]
pub struct ProviderSet {
    pub llm: Option<Arc<dyn LlmProvider>>,
    pub embedding: Option<Arc<dyn EmbeddingProvider>>,
    pub judge: Option<Arc<dyn JudgeProvider>>,
    pub extractor: Option<Arc<dyn EventExtractor>>,
    pub llm_requested: bool,
    pub embedding_requested: bool,
    pub judge_requested: bool,
    pub extractor_requested: bool,
    pub llm_error: Option<String>,
    pub embedding_error: Option<String>,
    pub judge_error: Option<String>,
    pub extractor_error: Option<String>,
}

impl ProviderSet {
    pub fn from_config(config: &ModelsConfig) -> Self {
        let mut set = Self {
            llm: None,
            embedding: None,
            judge: None,
            extractor: None,
            llm_requested: config.llm.enabled(),
            embedding_requested: config.embedding.enabled(),
            judge_requested: config.judge.enabled(),
            extractor_requested: config.extractor.enabled(),
            llm_error: None,
            embedding_error: None,
            judge_error: None,
            extractor_error: None,
        };

        if config.llm.enabled() {
            match OpenAiCompatible::from_config(&config.llm) {
                Ok(client) => set.llm = Some(Arc::new(client)),
                Err(err) => set.llm_error = Some(err.to_string()),
            }
        }
        if config.embedding.enabled() {
            match OpenAiCompatible::from_config(&config.embedding) {
                Ok(client) => set.embedding = Some(Arc::new(client)),
                Err(err) => set.embedding_error = Some(err.to_string()),
            }
        }
        if config.judge.enabled() {
            match OpenAiCompatible::from_config(&config.judge) {
                Ok(client) => set.judge = Some(Arc::new(client)),
                Err(err) => set.judge_error = Some(err.to_string()),
            }
        }
        if config.extractor.enabled() {
            match OpenAiCompatible::from_config(&config.extractor) {
                Ok(client) => set.extractor = Some(Arc::new(client)),
                Err(err) => set.extractor_error = Some(err.to_string()),
            }
        }
        set
    }

    pub fn with_none() -> Self {
        Self {
            llm: None,
            embedding: None,
            judge: None,
            extractor: None,
            llm_requested: false,
            embedding_requested: false,
            judge_requested: false,
            extractor_requested: false,
            llm_error: None,
            embedding_error: None,
            judge_error: None,
            extractor_error: None,
        }
    }

    pub fn embedding_available(&self) -> bool {
        self.embedding.is_some()
    }

    pub fn extractor_available(&self) -> bool {
        self.extractor.is_some()
    }
}

/// A deliberately small OpenAI-compatible client. "jev" and any other
/// provider name use the same wire shape; a Jev endpoint must therefore be
/// exposed through an OpenAI-compatible gateway. No provider is contacted
/// unless the user explicitly configures one.
pub struct OpenAiCompatible {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    max_retries: u32,
    id: String,
}

impl OpenAiCompatible {
    pub fn from_config(config: &ModelConfig) -> Result<Self, ProviderError> {
        let base_url = config
            .base_url_or_default()
            .ok_or_else(|| ProviderError::NotConfigured)?;
        let model = config
            .model
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ProviderError::InvalidResponse("model is required".into()))?;
        let api_key = config.api_key();
        if config.api_key_env.is_some() && api_key.is_none() {
            return Err(ProviderError::NotConfigured);
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms.max(1)))
            .build()
            .map_err(|err| ProviderError::Request(err.to_string()))?;
        Ok(Self {
            client,
            base_url,
            model,
            api_key,
            max_retries: config.max_retries,
            id: format!("{}:{}", config.provider, config.model.as_deref().unwrap_or("")),
        })
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, ProviderError> {
        let url = format!("{}{}", self.base_url.trim_end_matches('/'), path);
        let mut attempt = 0u32;
        loop {
            let mut request = self.client.post(&url).json(&body);
            if let Some(key) = &self.api_key {
                request = request.bearer_auth(key);
            }
            let outcome = request.send().await;
            match outcome {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        return response.json::<Value>().await.map_err(|err| {
                            ProviderError::InvalidResponse(err.to_string())
                        });
                    }
                    if status.is_server_error() && attempt < self.max_retries {
                        attempt += 1;
                        tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                        continue;
                    }
                    return Err(ProviderError::Request(format!(
                        "HTTP {} from {}",
                        status.as_u16(),
                        path
                    )));
                }
                Err(err) => {
                    if attempt < self.max_retries {
                        attempt += 1;
                        tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                        continue;
                    }
                    return Err(ProviderError::Request(err.to_string()));
                }
            }
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatible {
    async fn generate(&self, system: &str, user: &str) -> Result<String, ProviderError> {
        let body = json!({
            "model": self.model.clone(),
            "temperature": 0,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ]
        });
        let value = self.post("/chat/completions", body).await?;
        value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(|value| value.to_string())
            .ok_or_else(|| ProviderError::InvalidResponse("missing choices[0].message.content".into()))
    }

    fn id(&self) -> String {
        self.id.clone()
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiCompatible {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let body = json!({"model": self.model.clone(), "input": texts});
        let value = self.post("/embeddings", body).await?;
        let data = value
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::InvalidResponse("missing data array".into()))?;
        let mut parsed: Vec<(usize, Vec<f32>)> = Vec::new();
        for (fallback_index, item) in data.iter().enumerate() {
            let index = item
                .get("index")
                .and_then(Value::as_u64)
                .map(|value| value as usize)
                .unwrap_or(fallback_index);
            let embedding = item
                .get("embedding")
                .and_then(Value::as_array)
                .ok_or_else(|| ProviderError::InvalidResponse("missing embedding".into()))?
                .iter()
                .map(|value| value.as_f64().unwrap_or(0.0) as f32)
                .collect::<Vec<f32>>();
            parsed.push((index, embedding));
        }
        parsed.sort_by_key(|(index, _)| *index);
        Ok(parsed.into_iter().map(|(_, vector)| vector).collect())
    }

    fn id(&self) -> String {
        self.id.clone()
    }
}

#[async_trait]
impl JudgeProvider for OpenAiCompatible {
    async fn rerank(
        &self,
        query: &str,
        candidates: &[String],
    ) -> Result<Vec<f32>, ProviderError> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let mut listing = String::new();
        for (index, candidate) in candidates.iter().enumerate() {
            listing.push_str(&format!(
                "[{}] {}\n",
                index + 1,
                candidate.replace('\n', " ")
            ));
        }
        let system = "You are a retrieval judge. Score how well each candidate supports the query. \
Respond only with JSON: {\"scores\":[number,...]} in candidate order. Scores are 0..1.";
        let user = format!("Query: {query}\nCandidates:\n{listing}");
        let raw = self.generate(system, &user).await?;
        let parsed = parse_scores(&raw, candidates.len())?;
        Ok(parsed)
    }

    fn id(&self) -> String {
        self.id.clone()
    }
}

#[async_trait]
impl EventExtractor for OpenAiCompatible {
    async fn extract(
        &self,
        input: &ExtractionInput<'_>,
    ) -> Result<crate::event::ExtractedEvent, ProviderError> {
        let system = "Extract one semantic event and its named entities from the chunk. \
Return strict JSON only: {\"event\":\"...\",\"entities\":[{\"name\":\"...\",\"type\":\"...\"}]}. \
Do not invent entities; type is a short lowercase label such as concept, person, system, metric, or document.";
        let user = format!(
            "chunk_id: {}\ndoc_id: {}\nTEXT:\n{}",
            input.chunk_id, input.doc_id, input.text
        );
        let raw = self.generate(system, &user).await?;
        parse_extracted_event(&raw)
    }

    fn version(&self) -> String {
        crate::event::EXTRACTOR_VERSION.to_string()
    }

    fn model_id(&self) -> String {
        self.id.clone()
    }
}

fn parse_extracted_event(raw: &str) -> Result<crate::event::ExtractedEvent, ProviderError> {
    let text = crate::util::strip_code_fence(raw);
    let mut parsed: crate::event::ExtractedEvent = serde_json::from_str(text)
        .map_err(|err| ProviderError::InvalidResponse(format!("extractor JSON: {err}")))?;
    parsed.event = parsed.event.trim().to_string();
    if parsed.event.is_empty() {
        return Err(ProviderError::InvalidResponse(
            "extractor returned an empty event".into(),
        ));
    }
    let mut entities = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for entity in parsed.entities.into_iter().take(32) {
        let name = entity.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let entity_type = crate::event::normalize_entity_type(&entity.entity_type);
        let key = (crate::event::normalize_entity_name(&name), entity_type.clone());
        if seen.insert(key) {
            entities.push(crate::event::ExtractedEntity {
                name,
                entity_type,
            });
        }
    }
    parsed.entities = entities;
    Ok(parsed)
}

fn parse_scores(raw: &str, expected: usize) -> Result<Vec<f32>, ProviderError> {
    let text = crate::util::strip_code_fence(raw);
    let value: Value = serde_json::from_str(text)
        .map_err(|err| ProviderError::InvalidResponse(format!("judge JSON: {err}")))?;
    let array = value
        .get("scores")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
        .ok_or_else(|| ProviderError::InvalidResponse("judge response has no scores".into()))?;
    if array.len() != expected {
        return Err(ProviderError::InvalidResponse(format!(
            "judge returned {} scores for {expected} candidates",
            array.len()
        )));
    }
    Ok(array
        .iter()
        .map(|value| {
            value
                .as_f64()
                .or_else(|| value.as_i64().map(|v| v as f64))
                .unwrap_or(0.0) as f32
        })
        .collect())
}

/// Deterministic in-process providers for tests. They never open a socket and
/// are not used by the production wiring.
pub mod fake {
    use super::*;
    use async_trait::async_trait;

    pub struct FakeLlm {
        pub response: String,
        pub id: String,
    }

    impl FakeLlm {
        pub fn new(response: impl Into<String>) -> Self {
            Self {
                response: response.into(),
                id: "fake-llm".into(),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for FakeLlm {
        async fn generate(&self, _system: &str, _user: &str) -> Result<String, ProviderError> {
            Ok(self.response.clone())
        }

        fn id(&self) -> String {
            self.id.clone()
        }
    }

    pub struct FakeEmbedding {
        pub dims: usize,
        pub id: String,
    }

    impl FakeEmbedding {
        pub fn new(dims: usize) -> Self {
            Self {
                dims,
                id: format!("fake-embedding-{dims}"),
            }
        }
    }

    #[async_trait]
    impl EmbeddingProvider for FakeEmbedding {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ProviderError> {
            let mut out = Vec::new();
            for text in texts {
                let mut vector = vec![0.0f32; self.dims.max(1)];
                for (index, byte) in text.bytes().enumerate() {
                    let slot = (index + byte as usize) % vector.len();
                    vector[slot] += 1.0;
                }
                let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for value in &mut vector {
                        *value /= norm;
                    }
                }
                out.push(vector);
            }
            Ok(out)
        }

        fn id(&self) -> String {
            self.id.clone()
        }
    }

    pub struct FakeJudge {
        pub scores: Vec<f32>,
        pub id: String,
    }

    impl FakeJudge {
        pub fn new(scores: Vec<f32>) -> Self {
            Self {
                scores,
                id: "fake-judge".into(),
            }
        }
    }

    #[async_trait]
    impl JudgeProvider for FakeJudge {
        async fn rerank(
            &self,
            _query: &str,
            candidates: &[String],
        ) -> Result<Vec<f32>, ProviderError> {
            if self.scores.len() == candidates.len() {
                return Ok(self.scores.clone());
            }
            Ok((0..candidates.len()).map(|index| index as f32).collect())
        }

        fn id(&self) -> String {
            self.id.clone()
        }
    }

    pub struct FakeEventExtractor {
        pub id: String,
        pub fail: bool,
    }

    impl FakeEventExtractor {
        pub fn new() -> Self {
            Self {
                id: "fake-extractor".into(),
                fail: false,
            }
        }

        pub fn failing() -> Self {
            Self {
                id: "fake-extractor-failing".into(),
                fail: true,
            }
        }
    }

    impl Default for FakeEventExtractor {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl EventExtractor for FakeEventExtractor {
        async fn extract(
            &self,
            input: &ExtractionInput<'_>,
        ) -> Result<crate::event::ExtractedEvent, ProviderError> {
            if self.fail {
                return Err(ProviderError::Request(
                    "fake extractor configured to fail".into(),
                ));
            }
            let mut event = String::new();
            let mut rest = input.text;
            let mut entities = Vec::new();
            while let Some(start) = rest.find("[[") {
                event.push_str(&rest[..start]);
                let after = &rest[start + 2..];
                if let Some(end) = after.find("]]") {
                    let inner = &after[..end];
                    if let Some((name, entity_type)) = inner.split_once(':') {
                        let name = name.trim();
                        if !name.is_empty() {
                            entities.push(crate::event::ExtractedEntity {
                                name: name.to_string(),
                                entity_type: crate::event::normalize_entity_type(entity_type),
                            });
                        }
                    }
                    rest = &after[end + 2..];
                } else {
                    event.push_str("[[" );
                    rest = after;
                    break;
                }
            }
            event.push_str(rest);
            let event = event.trim().to_string();
            Ok(crate::event::ExtractedEvent {
                event: if event.is_empty() {
                    input.text.to_string()
                } else {
                    event
                },
                entities,
            })
        }

        fn version(&self) -> String {
            crate::event::EXTRACTOR_VERSION.to_string()
        }

        fn model_id(&self) -> String {
            self.id.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judge_scores_are_parsed_in_order() {
        let scores = parse_scores("{\"scores\":[0.1,0.9]}", 2).unwrap();
        assert_eq!(scores, vec![0.1, 0.9]);
        assert!(parse_scores("{\"scores\":[0.1]}", 2).is_err());
    }

    #[tokio::test]
    async fn fake_embedding_is_deterministic() {
        let provider = fake::FakeEmbedding::new(8);
        let a = provider.embed(&["hello".to_string()]).await.unwrap();
        let b = provider.embed(&["hello".to_string()]).await.unwrap();
        assert_eq!(a, b);
        assert_eq!(a[0].len(), 8);
    }

    #[tokio::test]
    async fn fake_extractor_parses_entity_markers() {
        let extractor = fake::FakeEventExtractor::new();
        let event = extractor
            .extract(&ExtractionInput {
                chunk_id: "chk",
                doc_id: "doc",
                text: "Alpha links Beta [[Alpha:Concept]] [[Beta:concept]]",
            })
            .await
            .unwrap();
        assert_eq!(event.entities.len(), 2);
        assert_eq!(event.entities[0].entity_type, "concept");
        assert!(event.event.contains("Alpha links Beta"));
        assert!(!event.event.contains("[["));
    }

    #[tokio::test]
    async fn fake_failing_extractor_returns_error() {
        let extractor = fake::FakeEventExtractor::failing();
        assert!(extractor
            .extract(&ExtractionInput {
                chunk_id: "chk",
                doc_id: "doc",
                text: "text",
            })
            .await
            .is_err());
    }
}
