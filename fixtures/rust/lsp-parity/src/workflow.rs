pub trait Named {
    fn name(&self) -> &str;
}

pub struct Job {
    name: String,
}

impl Job {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl Named for Job {
    fn name(&self) -> &str {
        &self.name
    }
}
