pub mod service;

pub use service::{DEFAULT_PREFIX, MemoryRepository, Service, build_service};

pub fn run_demo() -> String {
    let mut repository = MemoryRepository::default();
    repository.save("Ada");
    let service = build_service(repository);
    service.execute(" Grace ")
}
