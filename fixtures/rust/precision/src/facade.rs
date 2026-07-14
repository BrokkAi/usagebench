pub use crate::service::{Local, Worker};

pub type LocalAlias = Local;

pub fn build() -> Local {
    Local
}
