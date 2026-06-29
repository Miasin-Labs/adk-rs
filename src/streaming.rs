use crate::event::EventActions;
use crate::ids::InvocationId;
use crate::model::ModelResponse;

#[derive(Debug, Default, Clone)]
pub struct StreamingResponseAggregator {
    text: String,
}

impl StreamingResponseAggregator {
    pub fn push_partial_text(&mut self, chunk: &str) {
        self.text.push_str(chunk);
    }

    pub fn finish(self, _invocation_id: InvocationId) -> ModelResponse {
        ModelResponse {
            text: (!self.text.is_empty()).then_some(self.text),
            tool_calls: Vec::new(),
            actions: EventActions::default(),
        }
    }
}
