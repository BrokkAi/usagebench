pub const DEFAULT_PREFIX: &str = "job";

#[derive(Default)]
pub struct MemoryRepository {
    pub last: String,
}

impl MemoryRepository {
    pub fn save(&mut self, value: &str) -> String {
        self.last = value.to_string();
        value.trim().to_string()
    }
}

pub struct Service {
    repository: MemoryRepository,
}

impl Service {
    pub fn new(repository: MemoryRepository) -> Self {
        Self { repository }
    }

    pub fn execute(mut self, name: &str) -> String {
        let stored = self.repository.save(name);
        format!("{DEFAULT_PREFIX}:{stored}")
    }
}

pub fn build_service(repository: MemoryRepository) -> Service {
    Service::new(repository)
}
