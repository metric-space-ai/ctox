//! CTOX-owned web stack.
//!
//! This crate exists as an explicit compile and ownership boundary for the
//! browser/search/read surface so the root `ctox` binary only carries thin
//! adapters for those capabilities.

pub mod browser;
pub mod deep_research;
mod google_engine;
mod runtime_config;
pub mod surface;
pub mod web_search;

pub use browser::handle_browser_command;
pub use browser::prepare_browser_environment;
pub use browser::run_browser_automation;
pub use browser::BrowserAutomationRequest;
pub use browser::BrowserPrepareOptions;
pub use deep_research::run_ctox_deep_research_tool;
pub use deep_research::DeepResearchDepth;
pub use deep_research::DeepResearchRequest;
pub use surface::handle_web_command;
pub use surface::WebScrapeRequest;
pub use web_search::augment_responses_request;
pub use web_search::execute_canonical_web_search;
pub use web_search::run_ctox_google_bootstrap_doctor_tool;
pub use web_search::run_ctox_web_read_tool;
pub use web_search::run_ctox_web_search_tool;
pub use web_search::CanonicalWebSearchExecution;
pub use web_search::CanonicalWebSearchRequest;
pub use web_search::ContextSize;
pub use web_search::DirectWebReadRequest;
pub use web_search::OpenAiWebSearchCompatMode;
pub use web_search::SearchUserLocation;
