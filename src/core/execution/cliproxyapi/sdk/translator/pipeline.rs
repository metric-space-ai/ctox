// ref: sdk/translator/pipeline.go @ a88197f845c979132c8978ea223c6af05cc81536
// Port-Status: ported
// License: MIT (upstream); modifications AGPL-3.0-only

use super::{Format, Registry, TranslationContext, TranslationState};
use std::sync::Arc;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestEnvelope {
    pub format: Format,
    pub model: String,
    pub stream: bool,
    pub body: Vec<u8>,
    /// Whether a request normalizer or explicit envelope transform changed
    /// Responses configuration_update items for this request.
    pub configuration_updates_changed: bool,
}

/// Translates a request while preserving request-scoped metadata.
///
/// Upstream: `sdk/translator/types.go` `RequestEnvelopeTransform`.
pub type RequestEnvelopeTransform =
    Arc<dyn Fn(&TranslationContext, RequestEnvelope) -> RequestEnvelope + Send + Sync>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseEnvelope {
    pub format: Format,
    pub model: String,
    pub stream: bool,
    pub body: Vec<u8>,
    pub chunks: Vec<Vec<u8>>,
}

type RequestHandler = Arc<
    dyn Fn(&TranslationContext, RequestEnvelope) -> Result<RequestEnvelope, String> + Send + Sync,
>;
type ResponseHandler = Arc<
    dyn Fn(&TranslationContext, ResponseEnvelope) -> Result<ResponseEnvelope, String> + Send + Sync,
>;
pub type RequestMiddleware = Arc<
    dyn Fn(&TranslationContext, RequestEnvelope, RequestHandler) -> Result<RequestEnvelope, String>
        + Send
        + Sync,
>;
pub type ResponseMiddleware = Arc<
    dyn Fn(
            &TranslationContext,
            ResponseEnvelope,
            ResponseHandler,
        ) -> Result<ResponseEnvelope, String>
        + Send
        + Sync,
>;

pub struct Pipeline {
    registry: Arc<Registry>,
    request_middleware: Vec<RequestMiddleware>,
    response_middleware: Vec<ResponseMiddleware>,
}

impl Pipeline {
    pub fn new(registry: Arc<Registry>) -> Self {
        Self {
            registry,
            request_middleware: Vec::new(),
            response_middleware: Vec::new(),
        }
    }

    pub fn use_request(&mut self, middleware: RequestMiddleware) {
        self.request_middleware.push(middleware);
    }

    pub fn use_response(&mut self, middleware: ResponseMiddleware) {
        self.response_middleware.push(middleware);
    }

    pub fn translate_request(
        &self,
        context: &TranslationContext,
        from: Format,
        to: Format,
        request: RequestEnvelope,
    ) -> Result<RequestEnvelope, String> {
        let registry = self.registry.clone();
        let terminal: RequestHandler = Arc::new(move |context, request| {
            Ok(registry.translate_request_envelope(context, &from, &to, request))
        });
        let handler = self
            .request_middleware
            .iter()
            .rev()
            .fold(terminal, |next, middleware| {
                let middleware = middleware.clone();
                Arc::new(move |context, request| middleware(context, request, next.clone()))
            });
        handler(context, request)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn translate_response(
        &self,
        context: &TranslationContext,
        from: Format,
        to: Format,
        response: ResponseEnvelope,
        original_request: Vec<u8>,
        translated_request: Vec<u8>,
        state: TranslationState,
    ) -> Result<(ResponseEnvelope, TranslationState), String> {
        let registry = self.registry.clone();
        let shared_state = Arc::new(std::sync::Mutex::new(state));
        let terminal_state = shared_state.clone();
        let terminal: ResponseHandler = Arc::new(move |context, mut response| {
            let mut state = terminal_state
                .lock()
                .map_err(|_| "translation state poisoned".to_owned())?;
            if response.stream {
                response.chunks = registry.translate_stream(
                    context,
                    &from,
                    &to,
                    &response.model,
                    &original_request,
                    &translated_request,
                    &response.body,
                    &mut state,
                );
            } else {
                response.body = registry.translate_non_stream(
                    context,
                    &from,
                    &to,
                    &response.model,
                    &original_request,
                    &translated_request,
                    &response.body,
                    &mut state,
                );
            }
            response.format = to.clone();
            Ok(response)
        });
        let handler = self
            .response_middleware
            .iter()
            .rev()
            .fold(terminal, |next, middleware| {
                let middleware = middleware.clone();
                Arc::new(move |context, response| middleware(context, response, next.clone()))
            });
        let response = handler(context, response)?;
        drop(handler);
        let state = Arc::try_unwrap(shared_state)
            .map_err(|_| "translation state still shared".to_owned())?
            .into_inner()
            .map_err(|_| "translation state poisoned".to_owned())?;
        Ok((response, state))
    }
}
