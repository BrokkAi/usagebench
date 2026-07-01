pub mod workflow;

use workflow::{Job, Named};

pub trait Runner {
    type Output;

    fn run(job: Job) -> Self::Output {
        job.name().to_string()
    }
}

pub struct LocalRunner;

impl Runner for LocalRunner {
    type Output = String;
}

pub fn run_via_trait(job: Job) -> String {
    LocalRunner::run(job)
}

pub fn read_associated_output(value: <LocalRunner as Runner>::Output) -> String {
    value
}

macro_rules! define_job_maker {
    ($fn_name:ident) => {
        pub fn $fn_name() -> Job {
            Job::new("generated")
        }
    };
}

define_job_maker!(generated_job);

pub fn call_generated() -> Job {
    generated_job()
}
