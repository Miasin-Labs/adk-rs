use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::metric::{MetricEvaluator, MetricInput};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizationCandidate {
    pub prompt: String,
    pub score: f64,
}

#[async_trait]
pub trait Optimizer: Send + Sync {
    async fn optimize(&self, prompt: &str) -> Result<OptimizationCandidate, OptimizerError>;
}

#[derive(Debug, thiserror::Error)]
pub enum OptimizerError {
    #[error("optimization failed: {0}")]
    Failed(String),
    #[error("no candidate prompts to optimize over")]
    NoCandidates,
}

/// Produces the prompt variants to score for a given base prompt.
pub trait PromptVariants: Send + Sync {
    fn variants(&self, base_prompt: &str) -> Vec<String>;
}

impl<F> PromptVariants for F
where
    F: Fn(&str) -> Vec<String> + Send + Sync,
{
    fn variants(&self, base_prompt: &str) -> Vec<String> {
        self(base_prompt)
    }
}

/// A built-in [`Optimizer`] that scores each candidate prompt variant with a
/// [`MetricEvaluator`] and returns the highest-scoring one.
///
/// The base prompt is always included as a candidate, so optimization never
/// returns something worse than the input. Scoring is deterministic and needs
/// no live model: each variant is fed to the evaluator as the `actual` text
/// (with a fixed `MetricInput` template the caller supplies), and the variant
/// with the greatest [`MetricEvaluation::score`] wins.
pub struct MetricGuidedOptimizer {
    evaluator: Arc<dyn MetricEvaluator + Send + Sync>,
    variants: Arc<dyn PromptVariants>,
    template: MetricInput,
}

impl MetricGuidedOptimizer {
    pub fn new(
        evaluator: Arc<dyn MetricEvaluator + Send + Sync>,
        variants: Arc<dyn PromptVariants>,
    ) -> Self {
        Self {
            evaluator,
            variants,
            template: MetricInput {
                expected: String::new(),
                actual: String::new(),
                expected_tools: Vec::new(),
                actual_tools: Vec::new(),
                forbidden_terms: Vec::new(),
                grounded_terms: Vec::new(),
            },
        }
    }

    /// Use a non-default `MetricInput` template; the candidate prompt is placed
    /// in `actual` for each scoring call.
    pub fn with_template(mut self, template: MetricInput) -> Self {
        self.template = template;
        self
    }

    fn score(&self, prompt: &str) -> f64 {
        let mut input = self.template.clone();
        input.actual = prompt.to_owned();
        self.evaluator.evaluate(&input).score
    }
}

#[async_trait]
impl Optimizer for MetricGuidedOptimizer {
    async fn optimize(&self, prompt: &str) -> Result<OptimizationCandidate, OptimizerError> {
        // The base prompt is always a candidate, so we never regress below input.
        let mut candidates = self.variants.variants(prompt);
        candidates.push(prompt.to_owned());

        candidates
            .into_iter()
            .map(|candidate| {
                let score = self.score(&candidate);
                OptimizationCandidate {
                    prompt: candidate,
                    score,
                }
            })
            // Highest score wins; on ties, the later candidate (the base prompt
            // is pushed last) is kept via `>=`-style total_cmp ordering.
            .max_by(|a, b| a.score.total_cmp(&b.score))
            .ok_or(OptimizerError::NoCandidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::MetricEvaluation;

    /// Scores a prompt by its length (longer = higher), so the test can predict
    /// the winner deterministically.
    struct LengthEvaluator;

    impl MetricEvaluator for LengthEvaluator {
        fn evaluate(&self, input: &MetricInput) -> MetricEvaluation {
            MetricEvaluation {
                name: "length",
                score: input.actual.len() as f64,
                passed: !input.actual.is_empty(),
            }
        }
    }

    #[tokio::test]
    async fn metric_guided_optimizer_picks_highest_scoring_variant_normal() {
        let variants = Arc::new(|base: &str| {
            vec![
                format!("{base} please"),
                format!("{base} please, with detail and citations"),
                "x".to_owned(),
            ]
        });
        let optimizer =
            MetricGuidedOptimizer::new(Arc::new(LengthEvaluator), variants);

        let best = optimizer.optimize("answer").await.unwrap();
        // The longest variant must win under the length evaluator.
        assert_eq!(best.prompt, "answer please, with detail and citations");
        assert_eq!(best.score, best.prompt.len() as f64);
    }

    #[tokio::test]
    async fn metric_guided_optimizer_falls_back_to_base_prompt_robust() {
        // No extra variants: the base prompt is the only candidate and must win.
        let variants = Arc::new(|_: &str| Vec::<String>::new());
        let optimizer =
            MetricGuidedOptimizer::new(Arc::new(LengthEvaluator), variants);

        let best = optimizer.optimize("only-this").await.unwrap();
        assert_eq!(best.prompt, "only-this");
    }
}
