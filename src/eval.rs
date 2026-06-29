use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalMetric {
    pub name: String,
    pub threshold: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalCase {
    pub id: String,
    pub prompt: String,
    pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalResult {
    pub case_id: String,
    pub scores: BTreeMap<String, f64>,
    pub passed: bool,
}

pub trait EvalService: Send + Sync {
    fn put_case(&self, case: EvalCase) -> Result<(), EvalError>;
    fn list_cases(&self) -> Result<Vec<EvalCase>, EvalError>;
    fn record_result(&self, result: EvalResult) -> Result<(), EvalError>;
    fn list_results(&self, case_id: &str) -> Result<Vec<EvalResult>, EvalError>;
}

#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("eval store lock poisoned")]
    Poisoned,
    #[error("eval store I/O failed")]
    Io { source: std::io::Error },
    #[error("eval store JSON failed")]
    Json { source: serde_json::Error },
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryEvalService {
    cases: Arc<Mutex<BTreeMap<String, EvalCase>>>,
    results: Arc<Mutex<Vec<EvalResult>>>,
}

impl EvalService for InMemoryEvalService {
    fn put_case(&self, case: EvalCase) -> Result<(), EvalError> {
        let mut guard = self.cases.lock().map_err(|_| EvalError::Poisoned)?;
        guard.insert(case.id.clone(), case);
        Ok(())
    }

    fn list_cases(&self) -> Result<Vec<EvalCase>, EvalError> {
        let guard = self.cases.lock().map_err(|_| EvalError::Poisoned)?;
        Ok(guard.values().cloned().collect())
    }

    fn record_result(&self, result: EvalResult) -> Result<(), EvalError> {
        let mut guard = self.results.lock().map_err(|_| EvalError::Poisoned)?;
        guard.push(result);
        Ok(())
    }

    fn list_results(&self, case_id: &str) -> Result<Vec<EvalResult>, EvalError> {
        let guard = self.results.lock().map_err(|_| EvalError::Poisoned)?;
        Ok(guard
            .iter()
            .filter(|result| result.case_id == case_id)
            .cloned()
            .collect())
    }
}
