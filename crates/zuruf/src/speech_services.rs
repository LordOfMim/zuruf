//! Transcription services: what is usable here, private API keys, and the configured service.
//!
//! Keys live in `state/speech-keys.json` (owner-only). They are written by the settings view,
//! read only to talk to the service, and never returned by the API or written to a log.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use zuruf_core::{SettingsSpeech, SpeechProvider, SpeechService, SpeechServiceKey};
use zuruf_speech::Provider;

pub const ALL: [SpeechProvider; 8] = [
    SpeechProvider::Voxtype,
    SpeechProvider::WhisperCpp,
    SpeechProvider::Openai,
    SpeechProvider::Groq,
    SpeechProvider::Mistral,
    SpeechProvider::Deepgram,
    SpeechProvider::Elevenlabs,
    SpeechProvider::OpenaiCompatible,
];

pub fn provider(id: SpeechProvider) -> Provider {
    match id {
        SpeechProvider::Voxtype => Provider::Voxtype,
        SpeechProvider::WhisperCpp => Provider::WhisperCpp,
        SpeechProvider::Openai => Provider::Openai,
        SpeechProvider::Groq => Provider::Groq,
        SpeechProvider::Mistral => Provider::Mistral,
        SpeechProvider::Deepgram => Provider::Deepgram,
        SpeechProvider::Elevenlabs => Provider::Elevenlabs,
        SpeechProvider::OpenaiCompatible => Provider::OpenaiCompatible,
    }
}

pub fn parse(id: &str) -> Option<SpeechProvider> {
    ALL.into_iter().find(|p| provider(*p).id() == id)
}

/// Why a service cannot run on this platform at all.
fn unsupported(id: SpeechProvider) -> Option<&'static str> {
    if cfg!(windows) {
        return Some("Diktat ist unter Windows noch nicht verfügbar.");
    }
    if id == SpeechProvider::Voxtype && !cfg!(target_os = "linux") {
        return Some("Voxtype gibt es nur unter Linux.");
    }
    None
}

pub fn keys_path(state: &Path) -> PathBuf {
    state.join("speech-keys.json")
}

fn stored(path: &Path) -> BTreeMap<String, String> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write(path: &Path, keys: &BTreeMap<String, String>) -> std::io::Result<()> {
    if keys.is_empty() {
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    let bytes = serde_json::to_vec_pretty(keys).map_err(std::io::Error::other)?;
    zuruf_platform::atomic_write(path, &bytes)
}

pub fn store_key(path: &Path, id: SpeechProvider, key: &str) -> std::io::Result<()> {
    let mut keys = stored(path);
    let key = key.trim();
    if key.is_empty() {
        keys.remove(provider(id).id());
    } else {
        keys.insert(provider(id).id().into(), key.into());
    }
    write(path, &keys)
}

pub fn remove_key(path: &Path, id: SpeechProvider) -> std::io::Result<()> {
    let mut keys = stored(path);
    if keys.remove(provider(id).id()).is_some() {
        write(path, &keys)?;
    }
    Ok(())
}

/// The key to use: a stored one wins over the environment.
fn key(path: &Path, id: SpeechProvider) -> (Option<String>, SpeechServiceKey) {
    if let Some(key) = stored(path).remove(provider(id).id()) {
        return (Some(key), SpeechServiceKey::Stored);
    }
    let from_environment = provider(id)
        .info()
        .key_env
        .and_then(|name| std::env::var(name).ok())
        .filter(|value| !value.trim().is_empty());
    match from_environment {
        Some(key) => (Some(key), SpeechServiceKey::Environment),
        None => (None, SpeechServiceKey::None),
    }
}

pub fn describe(path: &Path, id: SpeechProvider) -> SpeechService {
    let info = provider(id).info();
    let reason = unsupported(id);
    SpeechService {
        id,
        label: info.label.into(),
        supported: reason.is_none(),
        unsupported_reason: reason.map(str::to_owned),
        local: info.local,
        needs_key: info.needs_key,
        key: key(path, id).1,
        key_env: info.key_env.map(str::to_owned),
        default_model: info.default_model.map(str::to_owned),
        default_endpoint: info.default_endpoint.map(str::to_owned),
    }
}

pub fn catalog(path: &Path) -> Vec<SpeechService> {
    ALL.into_iter().map(|id| describe(path, id)).collect()
}

fn filled(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

/// Why dictation with these settings cannot start, in words for the user; `None` when it can.
/// Voxtype reports its own problems when it starts, as before.
pub fn readiness(settings: &SettingsSpeech, keys: &Path) -> Option<String> {
    let id = settings.provider;
    if let Some(reason) = unsupported(id) {
        return Some(reason.into());
    }
    let info = provider(id).info();
    match id {
        SpeechProvider::Voxtype => None,
        SpeechProvider::WhisperCpp => match filled(&settings.model) {
            None => Some("whisper.cpp: Pfad zur Modelldatei (ggml-….bin) fehlt.".into()),
            Some(model) if !Path::new(&model).is_file() => {
                Some(format!("whisper.cpp: Modelldatei {model} nicht gefunden."))
            }
            Some(_) => None,
        },
        SpeechProvider::OpenaiCompatible if filled(&settings.endpoint).is_none() => {
            Some("Adresse des OpenAI-kompatiblen Dienstes fehlt.".into())
        }
        _ if info.needs_key && key(keys, id).0.is_none() => Some(format!(
            "{}: API-Schlüssel fehlt. Unter Einstellungen › Diktat hinterlegen.",
            info.label
        )),
        _ => None,
    }
}

/// The configured non-Voxtype service, ready to transcribe.
pub fn build(
    settings: &SettingsSpeech,
    keys: &Path,
    directory: &Path,
) -> Result<zuruf_speech::Service, String> {
    if let Some(reason) = readiness(settings, keys) {
        return Err(reason);
    }
    let id = settings.provider;
    Ok(match id {
        SpeechProvider::Voxtype => return Err("Voxtype wird gesondert gestartet.".into()),
        SpeechProvider::WhisperCpp => zuruf_speech::Service::WhisperCpp(zuruf_speech::WhisperCpp {
            executable: filled(&Some(settings.whisper_cpp_path.clone()))
                .unwrap_or_else(|| "whisper-cli".into())
                .into(),
            model: filled(&settings.model).unwrap_or_default().into(),
            language: filled(&settings.language),
            directory: directory.to_owned(),
            keep_audio: settings.keep_audio,
        }),
        _ => zuruf_speech::Service::Http(
            zuruf_speech::Http::new(
                provider(id),
                filled(&settings.endpoint),
                filled(&settings.model),
                filled(&settings.language),
                key(keys, id).0,
            )
            .map_err(|error| error.to_string())?,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    fn settings(provider: SpeechProvider) -> SettingsSpeech {
        let mut speech = zuruf_core::Settings::default().speech;
        speech.provider = provider;
        speech
    }

    #[test]
    fn keys_are_private_write_only_and_removable() {
        let directory = tempfile::tempdir().unwrap().keep();
        let path = keys_path(&directory);
        assert_eq!(
            describe(&path, SpeechProvider::Deepgram).key,
            SpeechServiceKey::None
        );
        store_key(&path, SpeechProvider::Deepgram, " dg-secret ").unwrap();
        let described = describe(&path, SpeechProvider::Deepgram);
        assert_eq!(described.key, SpeechServiceKey::Stored);
        assert!(
            !serde_json::to_string(&catalog(&path))
                .unwrap()
                .contains("dg-secret")
        );
        assert_eq!(
            key(&path, SpeechProvider::Deepgram).0.as_deref(),
            Some("dg-secret")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        remove_key(&path, SpeechProvider::Deepgram).unwrap();
        assert!(!path.exists());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn readiness_names_what_is_missing() {
        let keys = Path::new("/nonexistent/speech-keys.json");
        let missing = readiness(&settings(SpeechProvider::Elevenlabs), keys).unwrap();
        // The environment may hold a real key on a developer machine.
        if std::env::var_os("ELEVENLABS_API_KEY").is_none() {
            assert!(missing.contains("API-Schlüssel fehlt"), "{missing}");
        }
        let mut compatible = settings(SpeechProvider::OpenaiCompatible);
        assert!(readiness(&compatible, keys).unwrap().contains("Adresse"));
        compatible.endpoint = Some("http://127.0.0.1:8000/v1".into());
        assert_eq!(readiness(&compatible, keys), None);
        let mut whisper = settings(SpeechProvider::WhisperCpp);
        assert!(readiness(&whisper, keys).unwrap().contains("Modelldatei"));
        whisper.model = Some("/nonexistent/ggml-base.bin".into());
        assert!(
            readiness(&whisper, keys)
                .unwrap()
                .contains("nicht gefunden")
        );
        let voxtype = readiness(&settings(SpeechProvider::Voxtype), keys);
        assert_eq!(voxtype.is_none(), cfg!(target_os = "linux"));
    }

    #[test]
    fn ids_match_the_contract() {
        for id in ALL {
            let json = serde_json::to_value(id).unwrap();
            assert_eq!(json, provider(id).id());
            assert_eq!(parse(provider(id).id()), Some(id));
        }
    }
}
