// Origin: CTOX
// License: AGPL-3.0-only

mod init;
mod openai_openai_request;
mod openai_openai_response;

#[cfg(test)]
mod openai_openai_request_test;

pub use init::register_openai_chat_passthrough;
pub use openai_openai_request::convert_openai_request_to_openai;
pub use openai_openai_response::{
    convert_openai_response_to_openai, convert_openai_response_to_openai_non_stream,
    convert_openai_response_to_openai_with_state,
};
