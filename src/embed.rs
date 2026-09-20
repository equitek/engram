/// Embedding provider abstraction.
///
/// The vector space is model-specific: you must use the same provider and
/// model for both indexing and querying. Switching models requires a full
/// rebuild. The provider is stored in the index metadata so engram can
/// detect mismatches and prompt you to rebuild.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const DIMS_OPENAI_SMALL: usize = 1536;
pub const DIMS_NOMIC: usize = 768;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Provider {
    /// OpenAI text-embedding-3-small via API (requires OPENAI_API_KEY or OPENROUTER_API_KEY)
    OpenAiSmall,
    /// nomic-embed-text via local Ollama (no API key, privacy-preserving)
    OllamaNomic { base_url: String },
}

impl Provider {
    pub fn dims(&self) -> usize {
        match self {
            Provider::OpenAiSmall => DIMS_OPENAI_SMALL,
            Provider::OllamaNomic { .. } => DIMS_NOMIC,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Provider::OpenAiSmall => "openai/text-embedding-3-small",
            Provider::OllamaNomic { .. } => "ollama/nomic-embed-text",
        }
    }
}

pub fn embed(text: &str, provider: &Provider) -> Result<Vec<f32>> {
    // When ENGRAM_TEST_EMBED_FAIL=1, simulate a missing-model error
    if std::env::var("ENGRAM_TEST_EMBED_FAIL").as_deref() == Ok("1") {
        anyhow::bail!(
            "Ollama model 'nomic-embed-text' is not installed. \
             Run `ollama pull nomic-embed-text` and try again."
        );
    }

    // When ENGRAM_TEST_EMBED=1, skip HTTP and return a deterministic mock vector
    if std::env::var("ENGRAM_TEST_EMBED").as_deref() == Ok("1") {
        return Ok(mock_embedding(text));
    }

    match provider {
        Provider::OpenAiSmall => embed_openai(text),
        Provider::OllamaNomic { base_url } => embed_ollama(text, base_url),
    }
}

/// Deterministic 768-dim embedding from blake3 hash of input text.
///
/// Produces different vectors for different inputs without any HTTP calls.
/// Activated via `ENGRAM_TEST_EMBED=1` env var so tests can run without Ollama.
pub fn mock_embedding(text: &str) -> Vec<f32> {
    let hash = blake3::hash(text.as_bytes());
    let seed = hash.as_bytes();
    let mut out = Vec::with_capacity(DIMS_NOMIC);
    // Expand 32-byte hash into 768 floats using iterative blake3 hashing
    let mut block = *seed;
    for _ in 0..(DIMS_NOMIC / 8) {
        block = *blake3::hash(&block).as_bytes();
        for chunk in block.as_chunks::<4>().0 {
            let bits = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            // Map to [-1, 1] range
            out.push((bits as f32 / u32::MAX as f32) * 2.0 - 1.0);
        }
    }
    out.truncate(DIMS_NOMIC);
    out
}

fn embed_openai(text: &str) -> Result<Vec<f32>> {
    let api_key = std::env::var("OPENAI_API_KEY")
        .or_else(|_| std::env::var("OPENROUTER_API_KEY"))
        .context("Set OPENAI_API_KEY or OPENROUTER_API_KEY to use OpenAI embeddings")?;

    let base_url = if std::env::var("OPENROUTER_API_KEY").is_ok() {
        "https://openrouter.ai/api/v1"
    } else {
        "https://api.openai.com/v1"
    };

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(30))
        .build();

    let resp: serde_json::Value = agent
        .post(&format!("{base_url}/embeddings"))
        .set("Authorization", &format!("Bearer {api_key}"))
        .send_json(serde_json::json!({
            "model": "text-embedding-3-small",
            "input": text,
        }))?
        .into_json()?;

    parse_embedding(&resp)
}

fn embed_ollama(text: &str, base_url: &str) -> Result<Vec<f32>> {
    // Ollama ≥0.1.26 uses /api/embed (input key, embeddings[0] response).
    // Older versions use /api/embeddings (prompt key, embedding response).
    // Try new endpoint first; fall back on 404.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(5))
        .timeout_read(std::time::Duration::from_secs(60))
        .build();

    let new_url = format!("{base_url}/api/embed");
    match agent.post(&new_url).send_json(serde_json::json!({
        "model": "nomic-embed-text",
        "input": text,
    })) {
        Ok(resp) => {
            let body: serde_json::Value = resp.into_json()?;
            return body["embeddings"][0]
                .as_array()
                .context("No embeddings in Ollama /api/embed response")?
                .iter()
                .map(|v| {
                    v.as_f64()
                        .map(|f| f as f32)
                        .context("Non-numeric embedding value")
                })
                .collect();
        }
        Err(ureq::Error::Status(404, resp)) => {
            // Check if this is a model-not-found error vs endpoint-not-found.
            // Ollama returns {"error":"model \"X\" not found, ..."} when the
            // model hasn't been pulled.
            if let Ok(body) = resp.into_string() {
                if body.contains("not found") && body.contains("nomic-embed-text") {
                    anyhow::bail!(
                        "Ollama model 'nomic-embed-text' is not installed. \
                         Run `ollama pull nomic-embed-text` and try again."
                    );
                }
            }
            // Otherwise fall through to legacy endpoint
        }
        Err(e) => return Err(e.into()),
    }

    // Legacy: Ollama <0.1.26
    let resp: serde_json::Value = agent
        .post(&format!("{base_url}/api/embeddings"))
        .send_json(serde_json::json!({
            "model": "nomic-embed-text",
            "prompt": text,
        }))?
        .into_json()?;

    resp["embedding"]
        .as_array()
        .context("No embedding in Ollama /api/embeddings response")?
        .iter()
        .map(|v| {
            v.as_f64()
                .map(|f| f as f32)
                .context("Non-numeric embedding value")
        })
        .collect()
}

fn parse_embedding(resp: &serde_json::Value) -> Result<Vec<f32>> {
    resp["data"][0]["embedding"]
        .as_array()
        .context("No embedding in API response")?
        .iter()
        .map(|v| {
            v.as_f64()
                .map(|f| f as f32)
                .context("Non-numeric embedding value")
        })
        .collect()
}

/// Detect provider from environment. Prefers Ollama (local/private) when available.
pub fn detect_provider() -> Provider {
    // In test mode, return Ollama provider without probing the network
    if std::env::var("ENGRAM_TEST_EMBED").as_deref() == Ok("1") {
        return Provider::OllamaNomic {
            base_url: "http://localhost:11434".to_string(),
        };
    }

    // Check if local Ollama is up
    let base_url =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());

    let probe = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(2))
        .timeout_read(std::time::Duration::from_secs(2))
        .build();

    if probe
        .get(&format!("{base_url}/api/tags"))
        .call()
        .map(|r| r.status() == 200)
        .unwrap_or(false)
    {
        return Provider::OllamaNomic { base_url };
    }

    // Fall back to OpenAI-compatible
    Provider::OpenAiSmall
}
