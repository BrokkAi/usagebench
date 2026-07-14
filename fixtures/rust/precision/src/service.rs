pub trait Worker {
    fn run(&self);
}

pub struct Local;

impl Worker for Local {
    fn run(&self) {}
}
