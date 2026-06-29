use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CliCommand {
    Run { app: String, prompt: String },
    Web { port: u16 },
    ApiServer { port: u16 },
    Eval { eval_set: String },
    Create { template: String },
    Deploy { target: String },
}
