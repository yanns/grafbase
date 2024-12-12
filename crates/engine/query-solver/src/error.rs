#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Requirement cycle detected")]
    RequirementCycleDetected,
    #[error("Could not plan field: {name}")]
    CouldNotPlanField { name: String },
    #[error("Internal Error")]
    InternalError,
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
