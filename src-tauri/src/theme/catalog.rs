//! Provider presets and live model discovery.
//!
//! Two things live here, both deliberately boring:
//!
//! * [`PRESETS`] — compiled-in constants naming an endpoint a user can pick
//!   with one click. A preset is *not* user data, *not* theme data, and cannot
//!   be influenced by either. It exists so nobody has to paste a URL from a
//!   browser into a text field to try a free model.
//! * [`list_models`] — reads the provider's own `/models` endpoint so the UI
//!   can offer a selectable list instead of a text box the user has to guess
//!   into. OpenRouter publishes per-token pricing there, which is how free
//!   models are identified: nothing is hard-coded, so the list cannot go stale.
//!
//! ## The only rule that changed
//!
//! [`endpoint_allowed`] still refuses plain `http` for anything on the network,
//! because the API key travels in a header and would be readable on the wire.
//! It now *permits* `http` on the loopback interface, where there is no wire to
//! read: a model server on the same machine is a normal way to run things
//! offline, and refusing it never protected anyone.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::Serialize;

use crate::error::{AppError, AppResult, ErrorKind};

/// How long a fetched model list is reused before it is fetched again.
const MODEL_CACHE_TTL: Duration = Duration::from_secs(600);

/// How many models the UI is ever asked to render.
const MAX_MODELS: usize = 80;

/// A compiled-in endpoint preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub base_url: &'static str,
    /// One line for the UI, in the app's language.
    pub note: &'static str,
    /// True when `/models` reports per-token pricing (so "free" is knowable).
    pub lists_pricing: bool,
    /// True when the endpoint needs no key at all.
    pub keyless: bool,
}

/// The presets offered in the settings panel.
///
/// Ordered by how likely they are to be useful to someone who wants a free
/// model: OpenRouter first (many free models, no card), then the two other
/// hosted free tiers, then a local server for people who want no network at
/// all.
pub const PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "openrouter",
        label: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        note: "Бесплатные модели с суффиксом :free. Ограничение — 50 запросов в сутки, 1000 после разового пополнения на $10.",
        lists_pricing: true,
        keyless: false,
    },
    ProviderPreset {
        id: "groq",
        label: "Groq",
        base_url: "https://api.groq.com/openai/v1",
        note: "Быстрый бесплатный уровень, ключ выдаётся без карты.",
        lists_pricing: false,
        keyless: false,
    },
    ProviderPreset {
        id: "gemini",
        label: "Google AI Studio",
        base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
        note: "Совместимый с OpenAI вход к Gemini. Ключ бесплатный, но промпты могут использоваться для обучения вне ЕС.",
        lists_pricing: false,
        keyless: false,
    },
    ProviderPreset {
        id: "ollama",
        label: "Локально (Ollama)",
        base_url: "http://localhost:11434/v1",
        note: "Полностью офлайн, ключ не нужен. Работает только пока запущен Ollama на этом компьютере.",
        lists_pricing: false,
        keyless: true,
    },
];

/// One selectable model.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelOption {
    pub id: String,
    /// Human label, falling back to the id when the provider omits one.
    pub label: String,
    /// `Some(true)` when the provider states the model costs nothing,
    /// `Some(false)` when it states a price, `None` when it says neither.
    pub free: Option<bool>,
    /// Context window in tokens, when reported.
    pub context: Option<u64>,
}

/// Is this endpoint safe to talk to?
///
/// Any `https` endpoint is allowed. Plain `http` is allowed on the loopback
/// interface only, where the traffic never leaves the machine.
pub fn endpoint_allowed(url: &str) -> bool {
    let trimmed = url.trim();
    // Schemes are case-insensitive, so compare on the lowercased scheme only
    // and keep the rest of the URL as written.
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("https://") {
        return true;
    }
    let Some(rest) = lower.strip_prefix("http://") else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        // Drop any userinfo and the port.
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// A message worth showing a person, for a status code worth explaining.
///
/// The raw body still travels along as the error detail — it is the user's own
/// provider talking, and hiding it would make free-tier quotas impossible to
/// diagnose. This only replaces the part that would otherwise read as
/// "Модель вернула ошибку 429".
pub fn explain_status(status: u16) -> &'static str {
    match status {
        400 => "Модель не поняла запрос",
        401 => "Ключ не принят — проверьте его в настройках",
        402 => "У бесплатной модели закончился лимит или средства на счёте",
        403 => "Доступ к модели запрещён для этого ключа",
        404 => "Модель не найдена по этому адресу — возможно, её переименовали",
        408 => "Модель не ответила вовремя",
        429 => "Модель занята — бесплатные модели часто в очереди, повторите через минуту",
        s if (500..600).contains(&s) => "Сервис модели временно недоступен",
        _ => "Модель отклонила запрос",
    }
}

/// Parse a provider `/models` response into a sorted, capped option list.
///
/// Both shapes in the wild are accepted: OpenAI-style `{"data": [...]}` and a
/// bare array. Entries without an id are skipped rather than rendered as an
/// empty row. Free models come first, then alphabetical — a person looking for
/// something they can use without paying should not have to scroll.
pub fn parse_models(payload: &serde_json::Value) -> Vec<ModelOption> {
    let rows = payload
        .get("data")
        .and_then(|v| v.as_array())
        .or_else(|| payload.as_array());

    let Some(rows) = rows else {
        return Vec::new();
    };

    let mut options: Vec<ModelOption> = Vec::new();
    for row in rows {
        let Some(id) = row.get("id").and_then(|v| v.as_str()).map(str::trim) else {
            continue;
        };
        if id.is_empty() || options.iter().any(|o| o.id == id) {
            continue;
        }

        let label = row
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(id)
            .chars()
            .take(96)
            .collect();

        let context = row
            .get("context_length")
            .or_else(|| row.get("context_window"))
            .and_then(|v| v.as_u64());

        // Pricing is reported as decimal strings. Zero prompt *and* zero
        // completion is what "free" means; anything else is a paid model.
        let free = row
            .get("pricing")
            .and_then(|p| p.as_object())
            .and_then(|p| {
                let cost = |key: &str| -> Option<f64> {
                    p.get(key)
                        .and_then(|v| match v {
                            serde_json::Value::String(s) => s.parse::<f64>().ok(),
                            serde_json::Value::Number(n) => n.as_f64(),
                            _ => None,
                        })
                };
                let prompt = cost("prompt");
                let completion = cost("completion");
                match (prompt, completion) {
                    (Some(a), Some(b)) => Some(a == 0.0 && b == 0.0),
                    _ => None,
                }
            });

        options.push(ModelOption { id: id.to_string(), label, free, context });
    }

    options.sort_by(|a, b| {
        b.free
            .unwrap_or(false)
            .cmp(&a.free.unwrap_or(false))
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });
    options.truncate(MAX_MODELS);
    options
}

fn cache() -> &'static Mutex<HashMap<String, (Instant, Vec<ModelOption>)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Vec<ModelOption>)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Fetch the model list for an endpoint, reusing a recent result.
///
/// The API key is only attached when the caller says the endpoint matches the
/// stored configuration: the renderer never holds the key, so it cannot ask for
/// it to be sent somewhere new.
pub async fn list_models(
    base_url: &str,
    api_key: Option<&str>,
    http: &reqwest::Client,
) -> AppResult<Vec<ModelOption>> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(AppError::invalid("Сначала укажите адрес модели"));
    }
    if !endpoint_allowed(base) {
        return Err(AppError::config("Адрес должен использовать HTTPS (или быть локальным"));
    }

    if let Some((at, cached)) = cache().lock().get(base).cloned() {
        if at.elapsed() < MODEL_CACHE_TTL {
            return Ok(cached);
        }
    }

    let endpoint = format!("{base}/models");
    let mut builder = http
        .get(&endpoint)
        .timeout(Duration::from_secs(20))
        .header("Accept", "application/json");
    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        builder = builder.bearer_auth(key);
    }

    let response = builder.send().await.map_err(|e| {
        AppError::new(ErrorKind::Unreachable, "Не удалось получить список моделей")
            .with_detail(e.to_string())
            .retryable(true)
    })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(240).collect();
        return Err(
            AppError::new(ErrorKind::Network, explain_status(status.as_u16()))
                .with_detail(snippet)
                .retryable(status.is_server_error() || status.as_u16() == 429),
        );
    }

    let payload: serde_json::Value = response.json().await.map_err(|e| {
        AppError::new(ErrorKind::Network, "Провайдер вернул список моделей в неизвестном виде")
            .with_detail(e.to_string())
    })?;

    let models = parse_models(&payload);
    if models.is_empty() {
        return Err(AppError::new(
            ErrorKind::Network,
            "Провайдер не сообщил ни одной модели",
        ));
    }

    cache().lock().insert(base.to_string(), (Instant::now(), models.clone()));
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_point_somewhere_that_passes_our_own_rule() {
        for preset in PRESETS {
            assert!(
                endpoint_allowed(preset.base_url),
                "preset {} would be refused by our own check",
                preset.id
            );
        }
    }

    #[test]
    fn preset_ids_and_urls_are_unique() {
        let mut ids: Vec<&str> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate preset id");

        let mut urls: Vec<&str> = PRESETS.iter().map(|p| p.base_url).collect();
        urls.sort_unstable();
        let count = urls.len();
        urls.dedup();
        assert_eq!(urls.len(), count, "duplicate preset url");
    }

    #[test]
    fn https_is_allowed_and_remote_http_is_not() {
        assert!(endpoint_allowed("https://openrouter.ai/api/v1"));
        assert!(endpoint_allowed("HTTPS://example.com"));
        assert!(!endpoint_allowed("http://example.com/v1"));
        assert!(!endpoint_allowed("http://10.0.0.5:8080/v1"));
        assert!(!endpoint_allowed("ftp://example.com"));
        assert!(!endpoint_allowed("file:///etc/passwd"));
        assert!(!endpoint_allowed(""));
        // A hostname that merely starts with "localhost" is not loopback.
        assert!(!endpoint_allowed("http://localhost.evil.com/v1"));
    }

    #[test]
    fn loopback_http_is_allowed_for_a_local_model_server() {
        assert!(endpoint_allowed("http://localhost:11434/v1"));
        assert!(endpoint_allowed("http://127.0.0.1:1234/v1"));
        assert!(endpoint_allowed("http://user:pass@localhost:8080/v1"));
    }

    #[test]
    fn free_models_are_identified_from_pricing_and_sorted_first() {
        let payload = serde_json::json!({
            "data": [
                { "id": "paid/big", "name": "Big Paid", "pricing": { "prompt": "0.000003", "completion": "0.000015" } },
                { "id": "free/small:free", "name": "Small Free", "context_length": 131072, "pricing": { "prompt": "0", "completion": "0" } },
                { "id": "no/pricing" }
            ]
        });
        let models = parse_models(&payload);
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "free/small:free");
        assert_eq!(models[0].free, Some(true));
        assert_eq!(models[0].context, Some(131072));
        // The paid one is present but sorted after the free one.
        assert_eq!(models[1].id, "paid/big");
        assert_eq!(models[1].free, Some(false));
        // No pricing means "unknown", not "free".
        assert_eq!(models[2].free, None);
        assert_eq!(models[2].label, "no/pricing");
    }

    #[test]
    fn a_bare_array_is_accepted_too() {
        let payload = serde_json::json!([{ "id": "llama3.2:latest" }]);
        let models = parse_models(&payload);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "llama3.2:latest");
    }

    #[test]
    fn malformed_rows_are_skipped_rather_than_rendered_blank() {
        let payload = serde_json::json!({
            "data": [
                { "name": "no id here" },
                { "id": "   " },
                { "id": "good" },
                { "id": "good" }
            ]
        });
        let models = parse_models(&payload);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "good");
    }

    #[test]
    fn status_codes_become_sentences() {
        assert!(explain_status(429).contains("занята"));
        assert!(explain_status(401).contains("Ключ"));
        assert!(explain_status(404).contains("не найдена"));
        assert!(explain_status(503).contains("недоступен"));
        // Nothing panics on a code we did not enumerate.
        assert!(!explain_status(418).is_empty());
    }
}
