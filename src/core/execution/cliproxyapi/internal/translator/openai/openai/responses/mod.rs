// Origin: CTOX
// License: AGPL-3.0-only

#[path = "init.rs"]
mod init;
#[path = "openai_openai-responses_request.rs"]
mod request;
#[path = "openai_openai-responses_response.rs"]
mod response;
#[path = "openai_openai-responses_tools.rs"]
mod tools;

#[cfg(test)]
#[path = "openai_openai-responses_request_test.rs"]
mod request_test;
#[cfg(test)]
#[path = "openai_openai-responses_response_test.rs"]
mod response_test;

pub use init::register_openai_responses_chat_completions;
pub use request::convert_openai_responses_request_to_openai_chat_completions;
pub use response::{
    can_finalize_response_stream, convert_openai_chat_completions_response_to_openai_responses,
    convert_openai_chat_completions_response_to_openai_responses_non_stream,
};
