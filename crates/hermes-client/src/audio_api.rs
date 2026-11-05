//! Profile-scoped audio relay.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
pub const AUDIO_LIMIT: usize = 25 * 1024 * 1024;
pub const RESPONSE_LIMIT: usize = 40 * 1024 * 1024;
pub const DEADLINE: Duration = Duration::from_secs(120);

/// Audio requests use the credentials of the selected connection.
#[derive(Clone)]
pub enum AudioRelay {
    Local(Arc<crate::RestClient>),
    Remote {
        config: crate::remote::RemoteConfig,
        store: crate::remote::TokenStore,
    },
}

impl AudioRelay {
    async fn request(&self, profile: &str, path: &str, body: Value) -> Result<Value, String> {
        tokio::time::timeout(DEADLINE, async {
            match self {
                Self::Local(client) => {
                    let query = url::form_urlencoded::Serializer::new(String::new())
                        .append_pair("profile", profile)
                        .finish();
                    client
                        .request(
                            crate::Method::Post,
                            &format!("{path}?{query}"),
                            Some(&body),
                            crate::RequestOpts {
                                ticket: true,
                                deadline: Some(DEADLINE),
                            },
                        )
                        .await
                        .map_err(|_| "audio relay request failed".to_owned())
                }
                Self::Remote { config, store } => crate::remote::audio_request(
                    config,
                    store,
                    profile,
                    path,
                    body,
                    RESPONSE_LIMIT,
                    DEADLINE,
                )
                .await
                .map_err(|error| error.to_string()),
            }
        })
        .await
        .map_err(|_| "audio relay timed out".to_owned())?
    }

    pub async fn transcribe(&self, profile: &str, audio: &[u8]) -> Result<String, String> {
        let body = transcription_body(audio)?;
        let value = self.request(profile, "/api/audio/transcribe", body).await?;
        transcript(&value)
    }

    pub async fn speak(&self, profile: &str, text: &str) -> Result<Vec<u8>, String> {
        if text.len() > AUDIO_LIMIT {
            return Err("invalid text size".into());
        }
        let value = self
            .request(profile, "/api/audio/speak", json!({"text": text}))
            .await?;
        decode_audio(&value)
    }
}

fn transcription_body(audio: &[u8]) -> Result<Value, String> {
    if audio.is_empty() || audio.len() > AUDIO_LIMIT {
        return Err("invalid audio size".into());
    }
    Ok(
        json!({"data_url": format!("data:audio/wav;base64,{}", STANDARD.encode(audio)), "mime_type":"audio/wav"}),
    )
}

fn transcript(value: &Value) -> Result<String, String> {
    value["transcript"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "invalid audio relay response".into())
}

fn decode_audio(value: &Value) -> Result<Vec<u8>, String> {
    let data = value["data_url"]
        .as_str()
        .and_then(|value| value.split_once(","))
        .filter(|(header, _)| header.starts_with("data:audio/") && header.ends_with(";base64"))
        .ok_or_else(|| "invalid audio relay response".to_string())?
        .1;
    if data.len() > AUDIO_LIMIT.div_ceil(3) * 4 {
        return Err("invalid audio size".into());
    }
    let bytes = STANDARD
        .decode(data)
        .map_err(|_| "invalid audio relay response".to_string())?;
    if bytes.len() > AUDIO_LIMIT {
        return Err("invalid audio size".into());
    }
    Ok(bytes)
}
