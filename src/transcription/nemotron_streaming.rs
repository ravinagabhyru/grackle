//! Streaming provider for `parakeet-rs` Nemotron.
//!
//! Nemotron is a cache-aware English streaming ASR model. Unlike Parakeet EOU,
//! it does not emit an end-of-utterance marker; waystt keeps using its existing
//! silence detector to decide when to call `finalize_utterance`.

use std::path::Path;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

use super::{
    parakeet, ApiErrorDetails, StreamingSession, StreamingTranscriptionProvider, TranscriptionError,
};

const REQUIRED_SAMPLE_RATE: u32 = 16_000;
const NEMOTRON_CHUNK_SAMPLES: usize = 8960;

pub trait NemotronInference: Send {
    fn transcribe_chunk(&mut self, chunk: &[f32]) -> Result<String, TranscriptionError>;
    fn get_transcript(&mut self) -> Result<String, TranscriptionError>;
    fn reset(&mut self);
}

struct RealNemotron {
    model: parakeet_rs::Nemotron,
}

impl RealNemotron {
    fn from_handle(handle: &parakeet_rs::NemotronHandle) -> Self {
        Self {
            model: parakeet_rs::Nemotron::from_shared(handle),
        }
    }
}

impl NemotronInference for RealNemotron {
    fn transcribe_chunk(&mut self, chunk: &[f32]) -> Result<String, TranscriptionError> {
        self.model.transcribe_chunk(chunk).map_err(|e| {
            TranscriptionError::ApiError(ApiErrorDetails {
                provider: "Parakeet (Nemotron)".to_string(),
                status_code: None,
                error_code: Some("TRANSCRIPTION_ERROR".to_string()),
                error_message: format!("Nemotron streaming transcription failed: {e}"),
                raw_response: None,
            })
        })
    }

    fn get_transcript(&mut self) -> Result<String, TranscriptionError> {
        Ok(self.model.get_transcript())
    }

    fn reset(&mut self) {
        self.model.reset();
    }
}

#[derive(Clone)]
struct LoadedNemotronModel {
    handle: parakeet_rs::NemotronHandle,
}

pub struct NemotronStreamingProvider {
    model: Arc<LoadedNemotronModel>,
}

impl NemotronStreamingProvider {
    /// Load the shared Nemotron handle once. Each session created later gets
    /// its own independent decoder/cache state via `Nemotron::from_shared`.
    pub fn new(model_path: &Path) -> Result<Self, TranscriptionError> {
        if !model_path.exists() {
            return Err(TranscriptionError::ConfigurationError(format!(
                "Nemotron model directory not found: {}. Required files are: {}",
                model_path.display(),
                parakeet::NEMOTRON_REQUIRED_FILES.join(", ")
            )));
        }
        parakeet::validate_nemotron_model_dir(model_path)?;
        let path_str = model_path.to_str().ok_or_else(|| {
            TranscriptionError::ConfigurationError("Nemotron model path is not UTF-8".to_string())
        })?;
        let handle = parakeet_rs::NemotronHandle::load(path_str, None).map_err(|e| {
            TranscriptionError::ApiError(ApiErrorDetails {
                provider: "Parakeet (Nemotron)".to_string(),
                status_code: None,
                error_code: Some("MODEL_LOAD_ERROR".to_string()),
                error_message: format!("Failed to load Nemotron model: {e}"),
                raw_response: None,
            })
        })?;
        Ok(Self {
            model: Arc::new(LoadedNemotronModel { handle }),
        })
    }
}

#[async_trait]
impl StreamingTranscriptionProvider for NemotronStreamingProvider {
    async fn start_session(
        &self,
        sample_rate: u32,
    ) -> Result<Box<dyn StreamingSession>, TranscriptionError> {
        if sample_rate != REQUIRED_SAMPLE_RATE {
            return Err(TranscriptionError::ConfigurationError(format!(
                "Nemotron requires {REQUIRED_SAMPLE_RATE} Hz mono audio, got {sample_rate} Hz"
            )));
        }

        let handle = self.model.handle.clone();
        let session = NemotronSession::spawn(move || {
            Ok(Box::new(RealNemotron::from_handle(&handle)) as Box<dyn NemotronInference>)
        })
        .await?;
        Ok(Box::new(session))
    }
}

enum Cmd {
    Chunk {
        samples: Vec<f32>,
        reply: oneshot::Sender<Result<String, TranscriptionError>>,
    },
    Reset {
        reply: oneshot::Sender<()>,
    },
    SetPartialSink(mpsc::UnboundedSender<String>),
    Shutdown,
}

pub struct NemotronSession {
    cmd_tx: std_mpsc::Sender<Cmd>,
    thread: Mutex<Option<thread::JoinHandle<()>>>,
    carry_over: Vec<f32>,
    accumulator: String,
}

impl NemotronSession {
    async fn spawn<F>(build_inference: F) -> Result<Self, TranscriptionError>
    where
        F: FnOnce() -> Result<Box<dyn NemotronInference>, TranscriptionError> + Send + 'static,
    {
        let (cmd_tx, cmd_rx) = std_mpsc::channel::<Cmd>();
        let (ready_tx, ready_rx) = oneshot::channel::<Result<(), TranscriptionError>>();

        let handle = thread::Builder::new()
            .name("waystt-nemotron".into())
            .spawn(move || {
                let mut model = match build_inference() {
                    Ok(model) => {
                        let _ = ready_tx.send(Ok(()));
                        model
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                run_worker(&mut *model, &cmd_rx);
            })
            .map_err(|e| {
                TranscriptionError::ConfigurationError(format!(
                    "Failed to spawn Nemotron worker thread: {e}"
                ))
            })?;

        ready_rx.await.map_err(|_| {
            TranscriptionError::ConfigurationError(
                "Nemotron worker thread terminated before reporting readiness".to_string(),
            )
        })??;

        Ok(Self {
            cmd_tx,
            thread: Mutex::new(Some(handle)),
            carry_over: Vec::with_capacity(NEMOTRON_CHUNK_SAMPLES),
            accumulator: String::new(),
        })
    }

    #[cfg(test)]
    async fn spawn_with<F>(build_inference: F) -> Result<Self, TranscriptionError>
    where
        F: FnOnce() -> Result<Box<dyn NemotronInference>, TranscriptionError> + Send + 'static,
    {
        Self::spawn(build_inference).await
    }

    async fn send_chunk(&mut self, samples: Vec<f32>) -> Result<String, TranscriptionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(Cmd::Chunk {
                samples,
                reply: reply_tx,
            })
            .map_err(|_| {
                TranscriptionError::ConfigurationError(
                    "Nemotron worker thread has exited".to_string(),
                )
            })?;
        reply_rx.await.map_err(|_| {
            TranscriptionError::ConfigurationError(
                "Nemotron worker thread dropped reply channel".to_string(),
            )
        })?
    }

    fn install_partial_sink(&mut self, sink: mpsc::UnboundedSender<String>) {
        let _ = self.cmd_tx.send(Cmd::SetPartialSink(sink));
    }

    async fn send_reset(&mut self) -> Result<(), TranscriptionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.cmd_tx
            .send(Cmd::Reset { reply: reply_tx })
            .map_err(|_| {
                TranscriptionError::ConfigurationError(
                    "Nemotron worker thread has exited".to_string(),
                )
            })?;
        reply_rx.await.map_err(|_| {
            TranscriptionError::ConfigurationError(
                "Nemotron worker thread dropped reset reply channel".to_string(),
            )
        })
    }
}

impl Drop for NemotronSession {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(Cmd::Shutdown);
        if let Ok(mut guard) = self.thread.lock() {
            if let Some(handle) = guard.take() {
                let _ = handle.join();
            }
        }
    }
}

#[async_trait]
impl StreamingSession for NemotronSession {
    fn set_partial_sink(&mut self, sink: mpsc::UnboundedSender<String>) {
        self.install_partial_sink(sink);
    }

    async fn push_samples(&mut self, samples: &[f32]) -> Result<String, TranscriptionError> {
        self.carry_over.extend_from_slice(samples);

        while self.carry_over.len() >= NEMOTRON_CHUNK_SAMPLES {
            let chunk = self.carry_over.drain(..NEMOTRON_CHUNK_SAMPLES).collect();
            let delta = self.send_chunk(chunk).await?;
            self.accumulator.push_str(&delta);
        }

        Ok(String::new())
    }

    async fn finalize_utterance(&mut self) -> Result<String, TranscriptionError> {
        if !self.carry_over.is_empty() {
            let mut padded = std::mem::take(&mut self.carry_over);
            padded.resize(NEMOTRON_CHUNK_SAMPLES, 0.0);
            let delta = self.send_chunk(padded).await?;
            self.accumulator.push_str(&delta);
        }

        let mut out = std::mem::take(&mut self.accumulator);
        trim_trailing_whitespace(&mut out);
        self.send_reset().await?;
        Ok(out)
    }
}

fn run_worker(model: &mut dyn NemotronInference, cmd_rx: &std_mpsc::Receiver<Cmd>) {
    let mut partial_sink: Option<mpsc::UnboundedSender<String>> = None;
    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            Cmd::Chunk { samples, reply } => {
                let result = model.transcribe_chunk(&samples);
                if result.is_ok() {
                    if let Some(ref sink) = partial_sink {
                        if let Ok(full_hypothesis) = model.get_transcript() {
                            let _ = sink.send(full_hypothesis);
                        }
                    }
                }
                let _ = reply.send(result);
            }
            Cmd::Reset { reply } => {
                model.reset();
                let _ = reply.send(());
            }
            Cmd::SetPartialSink(sink) => {
                partial_sink = Some(sink);
            }
            Cmd::Shutdown => break,
        }
    }
}

fn trim_trailing_whitespace(text: &mut String) {
    while text.ends_with(|c: char| c.is_whitespace()) {
        text.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};

    struct StubNemotron {
        responses: Vec<Result<String, TranscriptionError>>,
        received_chunks: Arc<StdMutex<Vec<Vec<f32>>>>,
        reset_calls: Arc<StdMutex<u32>>,
    }

    impl NemotronInference for StubNemotron {
        fn transcribe_chunk(&mut self, chunk: &[f32]) -> Result<String, TranscriptionError> {
            self.received_chunks.lock().unwrap().push(chunk.to_vec());
            if self.responses.is_empty() {
                Ok(String::new())
            } else {
                let response = self.responses.remove(0);
                response
            }
        }

        fn get_transcript(&mut self) -> Result<String, TranscriptionError> {
            Ok(String::new())
        }

        fn reset(&mut self) {
            *self.reset_calls.lock().unwrap() += 1;
        }
    }

    async fn stub_session(
        responses: Vec<Result<String, TranscriptionError>>,
    ) -> (
        NemotronSession,
        Arc<StdMutex<Vec<Vec<f32>>>>,
        Arc<StdMutex<u32>>,
    ) {
        let received = Arc::new(StdMutex::new(Vec::new()));
        let resets = Arc::new(StdMutex::new(0u32));
        let received_clone = Arc::clone(&received);
        let reset_clone = Arc::clone(&resets);
        let session = NemotronSession::spawn_with(move || {
            Ok(Box::new(StubNemotron {
                responses,
                received_chunks: received_clone,
                reset_calls: reset_clone,
            }) as Box<dyn NemotronInference>)
        })
        .await
        .unwrap();
        (session, received, resets)
    }

    #[tokio::test]
    async fn test_push_samples_buffers_until_full_chunk() {
        let (mut session, received, resets) = stub_session(vec![]).await;
        let partial = vec![0.1f32; NEMOTRON_CHUNK_SAMPLES / 2];
        let out = session.push_samples(&partial).await.unwrap();
        assert_eq!(out, "");
        assert!(received.lock().unwrap().is_empty());
        assert_eq!(*resets.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn test_push_samples_accumulates_deltas_without_finalizing() {
        let (mut session, received, resets) =
            stub_session(vec![Ok("hello ".to_string()), Ok("world".to_string())]).await;
        let samples = vec![0.2f32; NEMOTRON_CHUNK_SAMPLES * 2];
        let out = session.push_samples(&samples).await.unwrap();
        assert_eq!(out, "");
        assert_eq!(received.lock().unwrap().len(), 2);
        assert_eq!(*resets.lock().unwrap(), 0);

        let final_text = session.finalize_utterance().await.unwrap();
        assert_eq!(final_text, "hello world");
        assert_eq!(*resets.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn test_finalize_pads_carry_over_and_resets() {
        let (mut session, received, resets) = stub_session(vec![Ok("tail ".to_string())]).await;
        let partial = vec![0.3f32; NEMOTRON_CHUNK_SAMPLES / 3];
        let _ = session.push_samples(&partial).await.unwrap();
        let out = session.finalize_utterance().await.unwrap();
        assert_eq!(out, "tail");

        let chunks = received.lock().unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), NEMOTRON_CHUNK_SAMPLES);
        assert!(chunks[0][partial.len()..].iter().all(|&s| s == 0.0));
        assert_eq!(*resets.lock().unwrap(), 1);
    }
}
