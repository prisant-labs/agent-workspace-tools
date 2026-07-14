use crate::error::Result;
use crate::model::{Change, Ctx, Hit, Move, Stale, Store, VerifyResult};

pub struct ClaudeProjects;

impl Store for ClaudeProjects {
    fn id(&self) -> &'static str {
        "claude.projects"
    }

    fn probe(&self, _ctx: &Ctx) -> Result<()> {
        Ok(())
    }

    fn detect(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<Hit>> {
        Ok(vec![])
    }

    fn audit(&self, _ctx: &Ctx) -> Result<Vec<Stale>> {
        Ok(vec![])
    }

    fn plan(&self, _ctx: &Ctx, _mv: &Move, _hit: &Hit) -> Result<Vec<Change>> {
        Ok(vec![])
    }

    fn verify(&self, _ctx: &Ctx, _mv: &Move) -> Result<Vec<VerifyResult>> {
        Ok(vec![])
    }
}
