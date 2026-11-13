//! Transcription services: Voxtype and whisper.cpp run locally, the others over HTTPS.
//!
//! Every service receives finished 16 kHz mono segments; recording and segmentation do not
//! depend on the choice. Silence never leaves the machine.
use crate::{AsyncResult, Error, Result, Transcriber, Voxtype, command, recorder::SAMPLE_RATE};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

/// The services Zuruf can send dictation to. Names match the settings contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Voxtype,
    WhisperCpp,
    Openai,
    Groq,
    Mistral,
    Deepgram,
    Elevenlabs,
    OpenaiCompatible,
}

/// Static facts about a service, shown in the settings and used as defaults.
#[derive(Debug, Clone, Copy)]
pub struct ProviderInfo {
    pub label: &'static str,
    pub default_model: Option<&'static str>,
    pub default_endpoint: Option<&'static str>,
    /// Environment variable that supplies a key when none is stored.
    pub key_env: Option<&'static str>,
    /// A key is required (an OpenAI-compatible local server usually needs none).
    pub needs_key: bool,
    /// Audio stays on this machine.
    pub local: bool,
}

impl Provider {
    pub const ALL: [Provider; 8] = [
        Provider::Voxtype,
        Provider::WhisperCpp,
        Provider::Openai,
        Provider::Groq,
        Provider::Mistral,
        Provider::Deepgram,
        Provider::Elevenlabs,
        Provider::OpenaiCompatible,
    ];

    pub fn info(self) -> ProviderInfo {
        let remote = |label, model, endpoint, key| ProviderInfo {
            label,
            default_model: Some(model),
            default_endpoint: Some(endpoint),
            key_env: Some(key),
            needs_key: true,
            local: false,
        };
        match self {
            Provider::Voxtype => ProviderInfo {
                label: "Voxtype",
                default_model: None,
                default_endpoint: None,
                key_env: None,
                needs_key: false,
                local: true,
            },
            Provider::WhisperCpp => ProviderInfo {
                label: "whisper.cpp",
                default_model: None,
                default_endpoint: None,
                key_env: None,
                needs_key: false,
                local: true,
            },
            Provider::Openai => remote(
                "OpenAI",
                "gpt-4o-mini-transcribe",
                "https://api.openai.com/v1",
                "OPENAI_API_KEY",
            ),
            Provider::Groq => remote(
                "Groq",
                "whisper-large-v3-turbo",
                "https://api.groq.com/openai/v1",
                "GROQ_API_KEY",
            ),
            Provider::Mistral => remote(
                "Mistral",
                "voxtral-mini-latest",
                "https://api.mistral.ai/v1",
                "MISTRAL_API_KEY",
            ),
            Provider::Deepgram => remote(
                "Deepgram",
                "nova-3",
                "https://api.deepgram.com/v1",
                "DEEPGRAM_API_KEY",
            ),
            Provider::Elevenlabs => remote(
                "ElevenLabs",
                "scribe_v2",
                "https://api.elevenlabs.io/v1",
                "ELEVENLABS_API_KEY",
            ),
            Provider::OpenaiCompatible => ProviderInfo {
                label: "OpenAI-kompatibel",
                default_model: Some("whisper-1"),
                default_endpoint: None,
                key_env: None,
                needs_key: false,
                local: false,
            },
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Provider::Voxtype => "voxtype",
            Provider::WhisperCpp => "whisper_cpp",
            Provider::Openai => "openai",
            Provider::Groq => "groq",
            Provider::Mistral => "mistral",
            Provider::Deepgram => "deepgram",
            Provider::Elevenlabs => "elevenlabs",
            Provider::OpenaiCompatible => "openai_compatible",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.id() == id)
    }

    /// Uses the HTTP transcription API of a cloud service or a compatible server.
    pub fn is_http(self) -> bool {
        !matches!(self, Provider::Voxtype | Provider::WhisperCpp)
    }
}

/// One configured service, ready to transcribe segments.
pub enum Service {
    Voxtype(Voxtype),
    WhisperCpp(WhisperCpp),
    Http(Http),
}

impl Transcriber for Service {
    fn transcribe(&self, samples: Vec<i16>) -> AsyncResult<'_, String> {
        match self {
            Service::Voxtype(service) => service.transcribe(samples),
            Service::WhisperCpp(service) => service.transcribe(samples),
            Service::Http(service) => service.transcribe(samples),
        }
    }
}

/// Encodes 16 kHz mono PCM as a WAV file in memory.
pub fn wav(samples: &[i16]) -> Result<Vec<u8>> {
    let mut bytes = std::io::Cursor::new(Vec::with_capacity(44 + samples.len() * 2));
    {
        let mut writer = hound::WavWriter::new(&mut bytes, wav_spec())?;
        for &sample in samples {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
    }
    Ok(bytes.into_inner())
}

pub(crate) fn wav_spec() -> hound::WavSpec {
    hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }
}

/// Writes a private WAV file for a command-line transcriber; returns its path.
pub(crate) async fn write_wav(directory: &std::path::Path, samples: Vec<i16>) -> Result<PathBuf> {
    zuruf_platform::private_dir(directory)?;
    let path = directory.join(format!("segment-{}.wav", uuid::Uuid::new_v4()));
    let bytes = tokio::task::spawn_blocking(move || wav(&samples))
        .await
        .map_err(|_| Error::Backend("WAV worker failed".into()))??;
    zuruf_platform::atomic_write(&path, &bytes)?;
    Ok(path)
}

/// whisper.cpp's command-line program (`whisper-cli`) with a local ggml model.
#[derive(Clone)]
pub struct WhisperCpp {
    pub executable: PathBuf,
    pub model: PathBuf,
    pub language: Option<String>,
    pub directory: PathBuf,
    pub keep_audio: bool,
}

impl Transcriber for WhisperCpp {
    fn transcribe(&self, samples: Vec<i16>) -> AsyncResult<'_, String> {
        Box::pin(async move {
            if crate::silent(&samples) {
                return Ok(String::new());
            }
            let path = write_wav(&self.directory, samples).await?;
            // whisper.cpp assumes English unless told otherwise.
            let language = self
                .language
                .as_deref()
                .map(short_language)
                .unwrap_or("auto");
            let result = command(&self.executable)
                .arg("--model")
                .arg(&self.model)
                .args(["--language", language, "--no-timestamps", "--no-prints"])
                .arg("--file")
                .arg(&path)
                .kill_on_drop(true)
                .output()
                .await;
            if !self.keep_audio {
                let _ = tokio::fs::remove_file(&path).await;
            }
            let output = result.map_err(|error| {
                Error::Backend(format!(
                    "{} konnte nicht gestartet werden: {error}",
                    self.executable.display()
                ))
            })?;
            if !output.status.success() {
                return Err(Error::Backend(format!(
                    "whisper.cpp beendet mit {:?}",
                    output.status.code()
                )));
            }
            Ok(join_lines(&String::from_utf8_lossy(&output.stdout)))
        })
    }
}

fn join_lines(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// "de-DE" → "de": the ISO 639-1 form most APIs expect.
fn short_language(language: &str) -> &str {
    language.split(['-', '_']).next().unwrap_or(language)
}

/// A cloud service or a server speaking one of their HTTP APIs.
pub struct Http {
    pub provider: Provider,
    pub endpoint: String,
    pub model: String,
    pub language: Option<String>,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl Http {
    /// Empty `endpoint` and `model` take the service's defaults. The key is never logged.
    pub fn new(
        provider: Provider,
        endpoint: Option<String>,
        model: Option<String>,
        language: Option<String>,
        api_key: Option<String>,
    ) -> Result<Self> {
        let info = provider.info();
        let endpoint = endpoint
            .filter(|e| !e.trim().is_empty())
            .or(info.default_endpoint.map(str::to_owned))
            .ok_or_else(|| Error::Backend("Adresse des Transkriptionsdienstes fehlt".into()))?;
        let model = model
            .filter(|m| !m.trim().is_empty())
            .or(info.default_model.map(str::to_owned))
            .ok_or_else(|| Error::Backend("Modell fehlt".into()))?;
        let api_key = api_key.filter(|k| !k.trim().is_empty());
        if info.needs_key && api_key.is_none() {
            return Err(Error::Backend(format!(
                "{}: API-Schlüssel fehlt",
                info.label
            )));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .user_agent(concat!("zuruf/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| Error::Backend(error.to_string()))?;
        Ok(Self {
            provider,
            endpoint: endpoint.trim().trim_end_matches('/').to_owned(),
            model: model.trim().to_owned(),
            language: language.filter(|l| !l.trim().is_empty()),
            api_key,
            client,
        })
    }

    fn request(&self, audio: Vec<u8>) -> Result<reqwest::RequestBuilder> {
        let file = || {
            reqwest::multipart::Part::bytes(audio.clone())
                .file_name("segment.wav")
                .mime_str("audio/wav")
                .map_err(|error| Error::Backend(error.to_string()))
        };
        let key = self.api_key.as_deref().unwrap_or_default();
        Ok(match self.provider {
            Provider::Deepgram => {
                let mut query = vec![
                    ("model", self.model.clone()),
                    ("smart_format", "true".into()),
                ];
                match &self.language {
                    Some(language) => query.push(("language", language.clone())),
                    None => query.push(("detect_language", "true".into())),
                }
                self.client
                    .post(format!("{}/listen", self.endpoint))
                    .query(&query)
                    .header("Authorization", format!("Token {key}"))
                    .header("Content-Type", "audio/wav")
                    .body(audio)
            }
            Provider::Elevenlabs => {
                let mut form = reqwest::multipart::Form::new()
                    .text("model_id", self.model.clone())
                    .text("tag_audio_events", "false")
                    .part("file", file()?);
                if let Some(language) = &self.language {
                    form = form.text("language_code", short_language(language).to_owned());
                }
                self.client
                    .post(format!("{}/speech-to-text", self.endpoint))
                    .header("xi-api-key", key)
                    .multipart(form)
            }
            _ => {
                let mut form = reqwest::multipart::Form::new()
                    .text("model", self.model.clone())
                    .text("response_format", "json")
                    .part("file", file()?);
                if let Some(language) = &self.language {
                    form = form.text("language", short_language(language).to_owned());
                }
                let request = self
                    .client
                    .post(format!("{}/audio/transcriptions", self.endpoint))
                    .multipart(form);
                if self.api_key.is_some() {
                    request.bearer_auth(key)
                } else {
                    request
                }
            }
        })
    }

    fn parse(&self, body: &serde_json::Value) -> Option<String> {
        let text = match self.provider {
            Provider::Deepgram => {
                body["results"]["channels"][0]["alternatives"][0]["transcript"].as_str()
            }
            _ => body["text"].as_str(),
        };
        text.map(|t| t.trim().to_owned())
    }
}

impl Transcriber for Http {
    fn transcribe(&self, samples: Vec<i16>) -> AsyncResult<'_, String> {
        Box::pin(async move {
            if crate::silent(&samples) {
                return Ok(String::new());
            }
            let audio = wav(&samples)?;
            let label = self.provider.info().label;
            let mut attempt = 0;
            loop {
                attempt += 1;
                let retry = attempt < 3;
                let response = match self.request(audio.clone())?.send().await {
                    Ok(response) => response,
                    Err(error) if retry && (error.is_connect() || error.is_timeout()) => {
                        tokio::time::sleep(Duration::from_millis(800 * attempt)).await;
                        continue;
                    }
                    Err(error) => {
                        // reqwest errors carry the URL but never headers, so no key leaks.
                        return Err(Error::Backend(format!("{label} nicht erreichbar: {error}")));
                    }
                };
                let status = response.status();
                if retry && (status.as_u16() == 429 || status.is_server_error()) {
                    tokio::time::sleep(Duration::from_millis(800 * attempt)).await;
                    continue;
                }
                let body = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    return Err(Error::Backend(match status.as_u16() {
                        401 | 403 => format!("{label}: API-Schlüssel abgelehnt"),
                        429 => format!("{label}: Ratenlimit oder Guthaben erschöpft"),
                        _ => format!("{label}: HTTP {} {}", status.as_u16(), excerpt(&body)),
                    }));
                }
                let json: serde_json::Value = serde_json::from_str(&body).map_err(|_| {
                    Error::Backend(format!("{label}: unerwartete Antwort {}", excerpt(&body)))
                })?;
                return self.parse(&json).ok_or_else(|| {
                    Error::Backend(format!("{label}: Antwort ohne Text {}", excerpt(&body)))
                });
            }
        })
    }
}

/// A short, single-line piece of an error body for the user.
fn excerpt(body: &str) -> String {
    let line: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut short: String = line.chars().take(160).collect();
    if short.len() < line.len() {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A one-request-at-a-time HTTP server that records what it was sent.
    async fn server(
        replies: Vec<(u16, &'static str)>,
    ) -> (String, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let task = tokio::spawn(async move {
            for (status, body) in replies {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 8192];
                loop {
                    let n = socket.read(&mut buffer).await.unwrap();
                    request.extend_from_slice(&buffer[..n]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(head) = text.find("\r\n\r\n") {
                        let length = text[..head]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if request.len() >= head + 4 + length {
                            break;
                        }
                    }
                }
                log.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&request).into_owned());
                let reply = format!(
                    "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                socket.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (address, seen, task)
    }

    fn speech() -> Vec<i16> {
        (0..16_000).map(|i| ((i % 40) as i16 - 20) * 400).collect()
    }

    #[tokio::test]
    async fn openai_compatible_multipart_and_bearer() {
        let (address, seen, task) = server(vec![(200, r#"{"text":" Hallo Welt. "}"#)]).await;
        let service = Http::new(
            Provider::Openai,
            Some(address),
            None,
            Some("de-DE".into()),
            Some("secret-key".into()),
        )
        .unwrap();
        assert_eq!(service.transcribe(speech()).await.unwrap(), "Hallo Welt.");
        task.await.unwrap();
        let request = &seen.lock().unwrap()[0];
        assert!(request.starts_with("POST /audio/transcriptions "));
        assert!(request.contains("authorization: Bearer secret-key"));
        assert!(request.contains("name=\"model\"\r\n\r\ngpt-4o-mini-transcribe"));
        assert!(request.contains("name=\"language\"\r\n\r\nde\r\n"));
        assert!(request.contains("filename=\"segment.wav\""));
        assert!(request.contains("RIFF"));
    }

    #[tokio::test]
    async fn deepgram_raw_body_and_detection() {
        let body = r#"{"results":{"channels":[{"alternatives":[{"transcript":"Guten Morgen"}]}]}}"#;
        let (address, seen, task) = server(vec![(503, "{}"), (200, body)]).await;
        let service = Http::new(
            Provider::Deepgram,
            Some(address),
            None,
            None,
            Some("dg".into()),
        )
        .unwrap();
        assert_eq!(service.transcribe(speech()).await.unwrap(), "Guten Morgen");
        task.await.unwrap();
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 2, "a server error is retried");
        assert!(
            requests[1]
                .starts_with("POST /listen?model=nova-3&smart_format=true&detect_language=true ")
        );
        assert!(requests[1].contains("authorization: Token dg"));
        assert!(requests[1].contains("content-type: audio/wav"));
    }

    #[tokio::test]
    async fn elevenlabs_header_and_rejected_key() {
        let (address, seen, task) = server(vec![(401, r#"{"detail":"bad"}"#)]).await;
        let service = Http::new(
            Provider::Elevenlabs,
            Some(address),
            Some("scribe_v1".into()),
            Some("de".into()),
            Some("xi".into()),
        )
        .unwrap();
        let error = service.transcribe(speech()).await.unwrap_err().to_string();
        assert!(error.contains("API-Schlüssel abgelehnt"), "{error}");
        assert!(!error.contains("xi"));
        task.await.unwrap();
        let request = &seen.lock().unwrap()[0];
        assert!(request.starts_with("POST /speech-to-text "));
        assert!(request.contains("xi-api-key: xi"));
        assert!(request.contains("name=\"model_id\"\r\n\r\nscribe_v1"));
        assert!(request.contains("name=\"language_code\"\r\n\r\nde\r\n"));
    }

    #[tokio::test]
    async fn silence_is_never_sent() {
        let service = Http::new(
            Provider::OpenaiCompatible,
            Some("http://127.0.0.1:9".into()),
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(service.transcribe(vec![0; 32_000]).await.unwrap(), "");
    }

    #[test]
    fn keys_and_endpoints_are_required_where_needed() {
        let missing_key = Http::new(Provider::Groq, None, None, None, Some(" ".into()));
        assert!(
            missing_key
                .err()
                .unwrap()
                .to_string()
                .contains("API-Schlüssel fehlt")
        );
        let missing_endpoint = Http::new(Provider::OpenaiCompatible, None, None, None, None);
        assert!(missing_endpoint.is_err());
        let groq = Http::new(Provider::Groq, None, None, None, Some("k".into())).unwrap();
        assert_eq!(groq.endpoint, "https://api.groq.com/openai/v1");
        assert_eq!(groq.model, "whisper-large-v3-turbo");
    }

    #[test]
    fn provider_ids_round_trip() {
        for provider in Provider::ALL {
            assert_eq!(Provider::from_id(provider.id()), Some(provider));
            let json = serde_json::to_value(provider).unwrap();
            assert_eq!(json, provider.id());
        }
    }

    #[test]
    fn wav_header_and_whisper_output() {
        let bytes = wav(&[1, -1]).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 48);
        assert_eq!(
            join_lines("\n [Musik] \n Hallo\n  Welt.\n"),
            "[Musik] Hallo Welt."
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn whisper_cpp_arguments() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile_dir();
        let program = directory.join("whisper-cli");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/args\"\nprintf '\\n Hallo aus whisper.cpp\\n'\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let service = WhisperCpp {
            executable: program,
            model: "/models/ggml-base.bin".into(),
            language: None,
            directory: directory.join("audio"),
            keep_audio: false,
        };
        assert_eq!(
            service.transcribe(speech()).await.unwrap(),
            "Hallo aus whisper.cpp"
        );
        let args = std::fs::read_to_string(directory.join("args")).unwrap();
        assert!(args.starts_with("--model\n/models/ggml-base.bin\n--language\nauto\n"));
        assert_eq!(
            std::fs::read_dir(directory.join("audio")).unwrap().count(),
            0
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn tempfile_dir() -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("zuruf-whisper-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        directory
    }
}
