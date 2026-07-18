use crate::model::{Applied, VerifyResult};
#[derive(Debug)]
pub struct Report {
    pub run_id: String,
    pub applied: Vec<Applied>,
    pub backup_dir: String,
    pub verify: Option<Vec<VerifyResult>>,
}
