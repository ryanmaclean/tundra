use crate::types::{FinishReason, StreamChunk, ToolCall};
use std::collections::HashMap;

/// Accumulates streaming chunks into a final response.
#[derive(Default)]
pub struct StreamAccumulator {
    pub content: String,
    tool_call_ids: HashMap<usize, String>,
    tool_call_names: HashMap<usize, String>,
    tool_call_args: HashMap<usize, String>,
    pub finish_reason: Option<FinishReason>,
}

impl StreamAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk into the accumulator.
    pub fn feed(&mut self, chunk: StreamChunk) {
        match chunk {
            StreamChunk::TextDelta(text) => {
                self.content.push_str(&text);
            }
            StreamChunk::ToolCallDelta {
                index,
                id,
                name,
                arguments_delta,
            } => {
                if let Some(id) = id {
                    self.tool_call_ids.insert(index, id);
                }
                if let Some(name) = name {
                    self.tool_call_names.insert(index, name);
                }
                self.tool_call_args
                    .entry(index)
                    .or_default()
                    .push_str(&arguments_delta);
            }
            StreamChunk::Done(reason) => {
                self.finish_reason = Some(reason);
            }
            StreamChunk::UsageInfo(_) => {}
        }
    }

    /// Extract the assembled tool calls.
    pub fn tool_calls(&self) -> Vec<ToolCall> {
        let mut calls = vec![];
        let mut indices: Vec<_> = self.tool_call_names.keys().collect();
        indices.sort();

        for idx in indices {
            let id = self.tool_call_ids.get(idx).cloned().unwrap_or_default();
            let name = self.tool_call_names.get(idx).cloned().unwrap_or_default();
            let args_str = self.tool_call_args.get(idx).cloned().unwrap_or_default();
            let arguments = serde_json::from_str(&args_str).unwrap_or(serde_json::Value::Null);

            calls.push(ToolCall {
                id,
                name,
                arguments,
            });
        }

        calls
    }
}
