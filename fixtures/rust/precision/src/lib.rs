mod service;
pub mod facade;
pub use facade::Worker;

pub fn consume() {
    let worker = facade::build();
    Worker::run(&worker);
    let other: facade::LocalAlias = facade::build();
    Worker::run(&other);
}
