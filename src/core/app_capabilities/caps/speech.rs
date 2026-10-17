//! CAS-only speech boundary; the existing native inference gateway executes.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Transcribe {
    #[serde(alias = "blob_id")]
    blob: blob::BlobRef,
    #[serde(default)]
    model: String,
    language: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Synthesize {
    text: String,
    voice: Option<String>,
    #[serde(default = "wav")]
    format: crate::execution::speech::SpeechAudioFormat,
}
fn wav() -> crate::execution::speech::SpeechAudioFormat {
    crate::execution::speech::SpeechAudioFormat::Wav
}
pub(super) fn execute(
    root: &Path,
    blobs: &dyn blob::BlobAccess,
    ctx: &CapCallContext,
    function: &str,
    args: &Value,
) -> Result<Value, CtoxError> {
    match function {
        "transcribe" => {
            let request: Transcribe = decode(args)?;
            if request.language.is_some() {
                return Err(invalid("$.language"));
            }
            blob::with_file(root, blobs, ctx, &request.blob, |path| {
                let text = crate::communication::gateway::transcribe_audio_file(
                    root,
                    path,
                    &request.model,
                )
                .map_err(native_error)?;
                Ok(json!({"text":text,"segments":[]}))
            })
        }
        "synthesize" => {
            let request: Synthesize = decode(args)?;
            let native = crate::execution::speech::SpeechRequest {
                text: request.text,
                voice_id: request.voice,
                format: request.format,
            };
            let gateway =
                crate::execution::speech::SpeechGateway::from_root(root).map_err(native_error)?;
            // Reuse the configured asynchronous adapter, including Computer.
            // Core runs this synchronous capability on its blocking executor.
            let runtime = tokio::runtime::Handle::try_current().map_err(native_error)?;
            if runtime.runtime_flavor() != tokio::runtime::RuntimeFlavor::MultiThread {
                return Err(native_error("Core multithread runtime required"));
            }
            let output = tokio::task::block_in_place(|| {
                runtime.block_on(gateway.synthesize_verified_async(&native))
            })
            .map_err(native_error)?;
            let mime = match output.format() {
                crate::execution::speech::SpeechAudioFormat::Wav => "audio/wav",
                crate::execution::speech::SpeechAudioFormat::Mp3 => "audio/mpeg",
                crate::execution::speech::SpeechAudioFormat::Pcm => "audio/pcm",
            };
            let reference = blob::publish(blobs, ctx, output.audio(), mime)?;
            Ok(
                json!({"blob_id":reference,"sha256":reference.sha256,"model":output.model(),"mime_type":mime}),
            )
        }
        _ => Err(CtoxError::new(
            ErrorCode::NotFound,
            "unknown speech function",
        )),
    }
}
