use super::NativeRoute;
use crate::app_capabilities::schema::PayloadShape;

pub(super) const ROUTES: &[NativeRoute] = &[NativeRoute::new(
    "schedule",
    "upsert",
    "news.topic.schedule",
    "business_os::store_news_commands::handle_news_control_command",
    "src/core/business_os/store_news_commands.rs:126",
    PayloadShape::Object,
)];
