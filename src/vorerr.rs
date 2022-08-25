use crate::config::{VORAppIdentifier, VORAppStatus};

pub struct VORAppError {
    pub id: i32,
    pub msg: String,
}

pub fn app_error(ai: i64, err_id: i32, msg: String) -> VORAppIdentifier {
    VORAppIdentifier {
        index: ai,
        status: VORAppStatus::AppError(VORAppError { id: err_id, msg }),
    }
}
