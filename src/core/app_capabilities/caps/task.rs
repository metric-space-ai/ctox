use super::NativeRoute;
use crate::app_capabilities::schema::PayloadShape;

pub(super) const ROUTES: &[NativeRoute] = &[
    NativeRoute::new(
        "agent.task",
        "delegate",
        "kundenpipeline.delegate",
        "business_os::decision_hub::handle_command",
        "src/core/business_os/decision_hub.rs:244",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.task",
        "company.research",
        "outbound.company.research",
        "business_os::store::record_command → importer::outbound_company_research",
        "src/core/business_os/store.rs:6753",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.task",
        "contact_research",
        "outbound.pipeline.contact_research",
        "business_os::store::record_command → importer::outbound_contact_research",
        "src/core/business_os/store.rs:6753",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.task",
        "lead_qualification",
        "outbound.pipeline.lead_qualification",
        "business_os::store::record_command → importer::outbound_lead_qualification",
        "src/core/business_os/store.rs:6753",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.task",
        "report_bug",
        "ctox.report.bug",
        "business_os::store::record_report_command",
        "src/core/business_os/store.rs:3949",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.task",
        "request",
        "threads.ai.request",
        "business_os::threads::handle_business_command → create_ai_request",
        "src/core/business_os/threads.rs:1090",
        PayloadShape::Object,
    ),
    NativeRoute::new(
        "agent.task",
        "approve",
        "threads.ctox_approval.approve",
        "business_os::threads::handle_business_command → approve_approval",
        "src/core/business_os/threads.rs:1090",
        PayloadShape::Object,
    ),
];
