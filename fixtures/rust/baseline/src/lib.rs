pub mod service;

pub use service::{
    AuditEvent, AuditFormatter, AuditLabel, DEFAULT_PREFIX, MemoryRepository, Service,
    build_service, format_audit_event,
};

pub fn run_demo() -> String {
    let mut repository = MemoryRepository::default();
    repository.save("Ada");
    let service = build_service(repository);
    let event = AuditEvent {
        label: AuditLabel {
            text: "batch".to_string(),
        },
    };
    let formatter: AuditFormatter = AuditFormatter::default();
    let audit = format_audit_event(event, formatter);
    format!("{}:{audit}", service.execute(" Grace "))
}
