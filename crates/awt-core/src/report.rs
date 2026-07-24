use crate::model::{Applied, VerifyResult};

#[derive(Debug)]
pub struct Report {
    pub run_id: String,
    pub applied: Vec<Applied>,
    pub backup_dir: String,
    pub verify: Option<Vec<VerifyResult>>,
}

impl Report {
    /// Serialize the report to a JSON object. Pure function: no IO.
    ///
    /// Shape:
    /// ```json
    /// {
    ///   "run_id": "...",
    ///   "applied": [{"change": "...", "counts": N}, ...],
    ///   "backup_dir": "awt-...",
    ///   "verify": [{"check": "...", "ok": true, "detail": "..."}] | null
    /// }
    /// ```
    pub fn to_json(&self) -> serde_json::Value {
        let applied: Vec<serde_json::Value> = self
            .applied
            .iter()
            .map(|a| {
                serde_json::json!({
                    "change": a.change,
                    "counts": a.counts,
                })
            })
            .collect();

        let verify = match &self.verify {
            None => serde_json::Value::Null,
            Some(results) => {
                let arr: Vec<serde_json::Value> = results
                    .iter()
                    .map(|v| {
                        serde_json::json!({
                            "check": v.check,
                            "ok": v.ok,
                            "detail": v.detail,
                        })
                    })
                    .collect();
                serde_json::Value::Array(arr)
            }
        };

        serde_json::json!({
            "run_id": self.run_id,
            "applied": applied,
            "backup_dir": self.backup_dir,
            "verify": verify,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Applied, VerifyResult};

    #[test]
    fn to_json_serializes_all_fields() {
        let report = Report {
            run_id: "test-run-1".into(),
            applied: vec![
                Applied {
                    change: "rename A -> B".into(),
                    counts: 0,
                },
                Applied {
                    change: "rewrite x.jsonl".into(),
                    counts: 3,
                },
            ],
            backup_dir: "awt-test-run-1".into(),
            verify: Some(vec![VerifyResult {
                check: "source_gone".into(),
                ok: true,
                detail: "ok".into(),
            }]),
        };

        let v = report.to_json();

        assert_eq!(v["run_id"], "test-run-1");
        assert_eq!(v["backup_dir"], "awt-test-run-1");
        let applied = v["applied"].as_array().unwrap();
        assert_eq!(applied.len(), 2);
        assert_eq!(applied[0]["change"], "rename A -> B");
        assert_eq!(applied[0]["counts"], 0);
        assert_eq!(applied[1]["change"], "rewrite x.jsonl");
        assert_eq!(applied[1]["counts"], 3);
        let verify = v["verify"].as_array().unwrap();
        assert_eq!(verify.len(), 1);
        assert_eq!(verify[0]["check"], "source_gone");
        assert_eq!(verify[0]["ok"], true);
        assert_eq!(verify[0]["detail"], "ok");
    }

    #[test]
    fn to_json_verify_is_null_when_none() {
        let report = Report {
            run_id: "x".into(),
            applied: vec![],
            backup_dir: "awt-x".into(),
            verify: None,
        };

        let v = report.to_json();
        assert!(v["verify"].is_null());
    }
}
