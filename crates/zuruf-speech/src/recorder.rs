//! Microphone capture into the shared 16 kHz mono sample clock.
//!
//! Linux records through PipeWire (`pw-record`), macOS through Core Audio. Both feed the same
//! clock, so segmentation and transcription do not depend on the platform.
use crate::Clock;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;

/// The rate every transcription service receives.
pub const SAMPLE_RATE: u32 = 16_000;

/// Appends captured audio to the clock and publishes the input level.
pub(crate) struct Feed {
    clock: Arc<Mutex<Clock>>,
    levels: watch::Sender<f32>,
    last_level: Instant,
    /// Samples seen before the first one that was not digital silence.
    leading_zeros: u64,
    heard: bool,
}

impl Feed {
    pub(crate) fn new(clock: Arc<Mutex<Clock>>, levels: watch::Sender<f32>) -> Self {
        Self {
            clock,
            levels,
            last_level: Instant::now(),
            leading_zeros: 0,
            heard: false,
        }
    }

    pub(crate) fn push(&mut self, samples: &[i16]) {
        if samples.is_empty() {
            return;
        }
        if self.last_level.elapsed() >= Duration::from_millis(66) {
            let power = samples
                .iter()
                .map(|s| (f32::from(*s) / 32768.0).powi(2))
                .sum::<f32>()
                / samples.len() as f32;
            let _ = self.levels.send(power.sqrt());
            self.last_level = Instant::now();
        }
        if !self.heard {
            if samples.iter().any(|&s| s != 0) {
                self.heard = true;
            } else {
                self.leading_zeros += samples.len() as u64;
            }
        }
        if let Ok(mut clock) = self.clock.lock() {
            clock.total += samples.len() as u64;
            clock.samples.extend_from_slice(samples);
        }
    }

    pub(crate) fn fail(&self, message: impl Into<String>) {
        if let Ok(mut clock) = self.clock.lock() {
            clock.failed.get_or_insert(message.into());
        }
    }

    /// True after six seconds of exact zeros: a real microphone always has some noise, so this
    /// is a muted input or (on macOS) a microphone permission that was refused. The margin
    /// leaves time to answer the system's permission prompt on first use.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn only_silence(&self) -> bool {
        !self.heard && self.leading_zeros >= u64::from(SAMPLE_RATE) * 6
    }
}

/// Mixes interleaved frames down to mono and resamples them to 16 kHz.
///
/// Every output sample is the mean of the input it covers, a box filter that keeps speech
/// intelligible without aliasing hiss when the device runs at 44.1 or 48 kHz.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) struct Resampler {
    channels: usize,
    /// Input samples per output sample.
    step: f64,
    position: f64,
    next: f64,
    sum: f64,
    count: u32,
    last: f32,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl Resampler {
    pub(crate) fn new(rate: u32, channels: usize) -> Self {
        let step = f64::from(rate.max(1)) / f64::from(SAMPLE_RATE);
        Self {
            channels: channels.max(1),
            step,
            position: 0.0,
            next: step,
            sum: 0.0,
            count: 0,
            last: 0.0,
        }
    }

    /// Takes interleaved samples in -1.0..=1.0.
    pub(crate) fn process(&mut self, interleaved: impl IntoIterator<Item = f32>) -> Vec<i16> {
        let mut output = Vec::new();
        let mut frame = 0.0f32;
        let mut channel = 0;
        for sample in interleaved {
            frame += sample;
            channel += 1;
            if channel < self.channels {
                continue;
            }
            let mono = frame / self.channels as f32;
            frame = 0.0;
            channel = 0;
            self.sum += f64::from(mono);
            self.count += 1;
            self.position += 1.0;
            while self.position >= self.next {
                if self.count > 0 {
                    self.last = (self.sum / f64::from(self.count)) as f32;
                }
                output.push((self.last.clamp(-1.0, 1.0) * 32767.0).round() as i16);
                self.sum = 0.0;
                self.count = 0;
                self.next += self.step;
            }
        }
        // Keep the counters small during long recordings.
        let whole = self.position.floor();
        self.position -= whole;
        self.next -= whole;
        output
    }
}

pub(crate) use platform::Capture;

#[cfg(not(target_os = "macos"))]
mod platform {
    use super::{Feed, SAMPLE_RATE};
    use crate::{Error, Result, command};
    use tokio::{io::AsyncReadExt, process::Child};

    /// A running `pw-record` and the task that reads its raw output.
    pub(crate) struct Capture {
        child: Child,
        reader: tokio::task::JoinHandle<()>,
    }

    impl Capture {
        pub(crate) async fn start(mut feed: Feed) -> Result<Self> {
            let rate = SAMPLE_RATE.to_string();
            let mut child = command("pw-record")
                .args([
                    "--raw",
                    "--rate",
                    &rate,
                    "--channels",
                    "1",
                    "--format",
                    "s16",
                    "-",
                ])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()?;
            let mut stdout = child
                .stdout
                .take()
                .ok_or_else(|| Error::Backend("no audio pipe".into()))?;
            let reader = tokio::spawn(async move {
                let mut buffer = [0; 4096];
                let mut carry = None;
                loop {
                    match stdout.read(&mut buffer).await {
                        Ok(0) => {
                            feed.fail("PipeWire stream ended");
                            break;
                        }
                        Ok(n) => {
                            let mut bytes = Vec::with_capacity(n + 1);
                            if let Some(b) = carry.take() {
                                bytes.push(b);
                            }
                            bytes.extend_from_slice(&buffer[..n]);
                            if bytes.len() % 2 != 0 {
                                carry = bytes.pop();
                            }
                            let samples: Vec<i16> = bytes
                                .as_chunks::<2>()
                                .0
                                .iter()
                                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                                .collect();
                            feed.push(&samples);
                        }
                        Err(error) => {
                            feed.fail(error.to_string());
                            break;
                        }
                    }
                }
            });
            Ok(Self { child, reader })
        }

        pub(crate) async fn stop(mut self) -> Result<()> {
            self.child.kill().await?;
            self.child.wait().await?;
            self.reader
                .await
                .map_err(|_| Error::Backend("capture task failed".into()))
        }

        pub(crate) fn abort(&mut self) {
            let _ = self.child.start_kill();
            self.reader.abort();
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{Feed, Resampler};
    use crate::{Error, Result};
    use cpal::{
        SampleFormat,
        traits::{DeviceTrait, HostTrait, StreamTrait},
    };
    use std::sync::{Arc, Mutex};

    const DENIED: &str = "Kein Zugriff auf das Mikrofon. Bitte unter Systemeinstellungen › Datenschutz & Sicherheit › Mikrofon für Zuruf erlauben.";

    /// Core Audio's stream is bound to the thread that created it, so a thread of its own owns
    /// it until `stop`.
    pub(crate) struct Capture {
        stop: Option<std::sync::mpsc::Sender<()>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    fn open(feed: Arc<Mutex<Feed>>) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| Error::Backend("Kein Mikrofon gefunden".into()))?;
        let supported = device.default_input_config().map_err(audio_error)?;
        let format = supported.sample_format();
        let config = supported.config();
        let mut resampler = Resampler::new(config.sample_rate, usize::from(config.channels));
        let failures = feed.clone();
        let on_error = move |error: cpal::Error| {
            if let Ok(feed) = failures.lock() {
                feed.fail(error.to_string());
            }
        };
        let deliver = move |samples: Vec<i16>| {
            if let Ok(mut feed) = feed.lock() {
                feed.push(&samples);
                if feed.only_silence() {
                    feed.fail(DENIED);
                }
            }
        };
        let stream = match format {
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _: &_| deliver(resampler.process(data.iter().copied())),
                on_error,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _: &_| {
                    deliver(resampler.process(data.iter().map(|&s| f32::from(s) / 32768.0)))
                },
                on_error,
                None,
            ),
            SampleFormat::I32 => device.build_input_stream(
                config,
                move |data: &[i32], _: &_| {
                    deliver(resampler.process(data.iter().map(|&s| s as f32 / 2_147_483_648.0)))
                },
                on_error,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                config,
                move |data: &[u16], _: &_| {
                    deliver(
                        resampler.process(data.iter().map(|&s| (f32::from(s) - 32768.0) / 32768.0)),
                    )
                },
                on_error,
                None,
            ),
            other => {
                return Err(Error::Backend(format!(
                    "Nicht unterstütztes Mikrofonformat {other:?}"
                )));
            }
        }
        .map_err(audio_error)?;
        stream.play().map_err(audio_error)?;
        Ok(stream)
    }

    fn audio_error(error: cpal::Error) -> Error {
        if error.kind() == cpal::ErrorKind::PermissionDenied {
            Error::Backend(DENIED.into())
        } else {
            Error::Backend(format!("Mikrofon: {error}"))
        }
    }

    impl Capture {
        pub(crate) async fn start(feed: Feed) -> Result<Self> {
            let feed = Arc::new(Mutex::new(feed));
            let (ready, opened) = tokio::sync::oneshot::channel();
            let (stop, stopped) = std::sync::mpsc::channel::<()>();
            let thread = std::thread::Builder::new()
                .name("zuruf-microphone".into())
                .spawn(move || match open(feed) {
                    Ok(stream) => {
                        let _ = ready.send(Ok(()));
                        // Either an explicit stop or a dropped sender ends the recording.
                        let _ = stopped.recv();
                        drop(stream);
                    }
                    Err(error) => {
                        let _ = ready.send(Err(error));
                    }
                })?;
            let mut capture = Self {
                stop: Some(stop),
                thread: Some(thread),
            };
            match opened.await {
                Ok(Ok(())) => Ok(capture),
                Ok(Err(error)) => {
                    capture.abort();
                    Err(error)
                }
                Err(_) => {
                    capture.abort();
                    Err(Error::Backend("Mikrofon-Thread beendet".into()))
                }
            }
        }

        pub(crate) async fn stop(mut self) -> Result<()> {
            self.stop.take();
            if let Some(thread) = self.thread.take() {
                tokio::task::spawn_blocking(move || thread.join())
                    .await
                    .map_err(|_| Error::Backend("capture task failed".into()))?
                    .map_err(|_| Error::Backend("capture thread panicked".into()))?;
            }
            Ok(())
        }

        pub(crate) fn abort(&mut self) {
            // The thread ends on its own once the sender is gone.
            self.stop.take();
            self.thread.take();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmixes_and_resamples_to_sixteen_kilohertz() {
        let mut resampler = Resampler::new(48_000, 2);
        // One second of a constant stereo signal: left 0.5, right 0.0.
        let input = (0..48_000).flat_map(|_| [0.5f32, 0.0]);
        let output = resampler.process(input);
        assert_eq!(output.len(), 16_000);
        assert!(
            output.iter().all(|&s| (s - 8192).abs() <= 1),
            "{:?}",
            &output[..4]
        );
    }

    #[test]
    fn handles_fractional_ratios_across_chunks() {
        let mut resampler = Resampler::new(44_100, 1);
        let mut total = 0;
        for _ in 0..100 {
            total += resampler.process(std::iter::repeat_n(0.25f32, 441)).len();
        }
        // 44 100 input samples are one second.
        assert!((15_999..=16_001).contains(&total), "{total}");
    }

    #[test]
    fn upsamples_slow_devices() {
        let mut resampler = Resampler::new(8_000, 1);
        assert_eq!(
            resampler.process(std::iter::repeat_n(0.0f32, 8_000)).len(),
            16_000
        );
    }

    #[test]
    fn silence_detection_needs_six_seconds_of_exact_zeros() {
        let clock = Arc::new(Mutex::new(Clock::default()));
        let (levels, _) = watch::channel(0.0);
        let mut feed = Feed::new(clock.clone(), levels);
        feed.push(&[0; 80_000]);
        assert!(!feed.only_silence());
        feed.push(&[0; 16_000]);
        assert!(feed.only_silence());
        let mut feed = Feed::new(clock.clone(), watch::channel(0.0).0);
        feed.push(&[0, 1]);
        feed.push(&[0; 128_000]);
        assert!(!feed.only_silence());
        assert_eq!(clock.lock().unwrap().total, 224_002);
    }
}
