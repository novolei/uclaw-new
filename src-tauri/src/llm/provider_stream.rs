use serde::{Deserialize, Serialize};

use crate::agent::types::{StreamDelta, TokenUsage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStreamEventKind {
    Start,
    TextStart,
    TextDelta,
    TextEnd,
    ThinkingStart,
    ThinkingDelta,
    ThinkingSignature,
    ThinkingEnd,
    ToolCallStart,
    ToolCallDelta,
    ToolCallEnd,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ProviderStreamEvent {
    Start,
    TextStart {
        content_index: usize,
    },
    TextDelta {
        content_index: usize,
        delta: String,
    },
    TextEnd {
        content_index: usize,
        content: String,
    },
    ThinkingStart {
        content_index: usize,
    },
    ThinkingDelta {
        content_index: usize,
        delta: String,
    },
    ThinkingSignature {
        content_index: usize,
        signature: String,
    },
    ThinkingEnd {
        content_index: usize,
        content: String,
    },
    ToolCallStart {
        content_index: usize,
        id: String,
        name: Option<String>,
    },
    ToolCallDelta {
        content_index: usize,
        delta: String,
    },
    ToolCallEnd {
        content_index: usize,
        id: String,
        name: Option<String>,
        input_json: String,
    },
    Done {
        finish_reason: Option<String>,
        usage: Option<TokenUsage>,
    },
}

impl ProviderStreamEvent {
    pub const fn kind(&self) -> ProviderStreamEventKind {
        match self {
            Self::Start => ProviderStreamEventKind::Start,
            Self::TextStart { .. } => ProviderStreamEventKind::TextStart,
            Self::TextDelta { .. } => ProviderStreamEventKind::TextDelta,
            Self::TextEnd { .. } => ProviderStreamEventKind::TextEnd,
            Self::ThinkingStart { .. } => ProviderStreamEventKind::ThinkingStart,
            Self::ThinkingDelta { .. } => ProviderStreamEventKind::ThinkingDelta,
            Self::ThinkingSignature { .. } => ProviderStreamEventKind::ThinkingSignature,
            Self::ThinkingEnd { .. } => ProviderStreamEventKind::ThinkingEnd,
            Self::ToolCallStart { .. } => ProviderStreamEventKind::ToolCallStart,
            Self::ToolCallDelta { .. } => ProviderStreamEventKind::ToolCallDelta,
            Self::ToolCallEnd { .. } => ProviderStreamEventKind::ToolCallEnd,
            Self::Done { .. } => ProviderStreamEventKind::Done,
        }
    }
}

#[derive(Debug, Default)]
pub struct ProviderStreamAssembler {
    started: bool,
    terminated: bool,
    next_content_index: usize,
    active: Option<ActiveBlock>,
}

#[derive(Debug)]
enum ActiveBlock {
    Text {
        content_index: usize,
        content: String,
    },
    Thinking {
        content_index: usize,
        content: String,
    },
    ToolCall {
        content_index: usize,
        id: String,
        name: Option<String>,
        input_json: String,
    },
}

impl ProviderStreamAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_delta(&mut self, delta: StreamDelta) -> Vec<ProviderStreamEvent> {
        if self.terminated {
            return Vec::new();
        }

        let mut events = Vec::new();
        self.ensure_started(&mut events);

        match delta {
            StreamDelta::TextDelta { text } => self.push_text_delta(text, &mut events),
            StreamDelta::ThinkingDelta { thinking } => {
                self.push_thinking_delta(thinking, &mut events);
            }
            StreamDelta::SignatureDelta { signature } => {
                self.push_thinking_signature(signature, &mut events);
            }
            StreamDelta::ToolCallDelta {
                id,
                name,
                input_json,
            } => self.push_tool_call_delta(id, name, input_json, &mut events),
            StreamDelta::Done {
                finish_reason,
                usage,
            } => {
                self.close_active(&mut events);
                events.push(ProviderStreamEvent::Done {
                    finish_reason,
                    usage,
                });
                self.terminated = true;
            }
        }

        events
    }

    pub fn finish(&mut self) -> Vec<ProviderStreamEvent> {
        if self.terminated || !self.started {
            return Vec::new();
        }
        let mut events = Vec::new();
        self.close_active(&mut events);
        self.terminated = true;
        events
    }

    fn ensure_started(&mut self, events: &mut Vec<ProviderStreamEvent>) {
        if !self.started {
            self.started = true;
            events.push(ProviderStreamEvent::Start);
        }
    }

    fn allocate_content_index(&mut self) -> usize {
        let index = self.next_content_index;
        self.next_content_index += 1;
        index
    }

    fn push_text_delta(&mut self, text: String, events: &mut Vec<ProviderStreamEvent>) {
        if !matches!(self.active, Some(ActiveBlock::Text { .. })) {
            self.close_active(events);
            let content_index = self.allocate_content_index();
            self.active = Some(ActiveBlock::Text {
                content_index,
                content: String::new(),
            });
            events.push(ProviderStreamEvent::TextStart { content_index });
        }

        if let Some(ActiveBlock::Text {
            content_index,
            content,
        }) = &mut self.active
        {
            content.push_str(&text);
            events.push(ProviderStreamEvent::TextDelta {
                content_index: *content_index,
                delta: text,
            });
        }
    }

    fn push_thinking_delta(&mut self, thinking: String, events: &mut Vec<ProviderStreamEvent>) {
        if !matches!(self.active, Some(ActiveBlock::Thinking { .. })) {
            self.close_active(events);
            let content_index = self.allocate_content_index();
            self.active = Some(ActiveBlock::Thinking {
                content_index,
                content: String::new(),
            });
            events.push(ProviderStreamEvent::ThinkingStart { content_index });
        }

        if let Some(ActiveBlock::Thinking {
            content_index,
            content,
        }) = &mut self.active
        {
            content.push_str(&thinking);
            events.push(ProviderStreamEvent::ThinkingDelta {
                content_index: *content_index,
                delta: thinking,
            });
        }
    }

    fn push_thinking_signature(
        &mut self,
        signature: String,
        events: &mut Vec<ProviderStreamEvent>,
    ) {
        if !matches!(self.active, Some(ActiveBlock::Thinking { .. })) {
            self.close_active(events);
            let content_index = self.allocate_content_index();
            self.active = Some(ActiveBlock::Thinking {
                content_index,
                content: String::new(),
            });
            events.push(ProviderStreamEvent::ThinkingStart { content_index });
        }

        if let Some(ActiveBlock::Thinking { content_index, .. }) = &self.active {
            events.push(ProviderStreamEvent::ThinkingSignature {
                content_index: *content_index,
                signature,
            });
        }
    }

    fn push_tool_call_delta(
        &mut self,
        id: String,
        name: Option<String>,
        input_json: Option<String>,
        events: &mut Vec<ProviderStreamEvent>,
    ) {
        let starts_new_tool = match &self.active {
            Some(ActiveBlock::ToolCall {
                id: active_id,
                name: active_name,
                ..
            }) => name.is_some() || active_id != &id || active_name.is_none(),
            _ => true,
        };

        if starts_new_tool {
            self.close_active(events);
            let content_index = self.allocate_content_index();
            events.push(ProviderStreamEvent::ToolCallStart {
                content_index,
                id: id.clone(),
                name: name.clone(),
            });
            self.active = Some(ActiveBlock::ToolCall {
                content_index,
                id,
                name,
                input_json: String::new(),
            });
        } else if let Some(ActiveBlock::ToolCall {
            name: active_name, ..
        }) = &mut self.active
        {
            if active_name.is_none() {
                *active_name = name;
            }
        }

        if let Some(delta) = input_json {
            if let Some(ActiveBlock::ToolCall {
                content_index,
                input_json,
                ..
            }) = &mut self.active
            {
                input_json.push_str(&delta);
                events.push(ProviderStreamEvent::ToolCallDelta {
                    content_index: *content_index,
                    delta,
                });
            }
        }
    }

    fn close_active(&mut self, events: &mut Vec<ProviderStreamEvent>) {
        match self.active.take() {
            Some(ActiveBlock::Text {
                content_index,
                content,
            }) => events.push(ProviderStreamEvent::TextEnd {
                content_index,
                content,
            }),
            Some(ActiveBlock::Thinking {
                content_index,
                content,
            }) => events.push(ProviderStreamEvent::ThinkingEnd {
                content_index,
                content,
            }),
            Some(ActiveBlock::ToolCall {
                content_index,
                id,
                name,
                input_json,
            }) => events.push(ProviderStreamEvent::ToolCallEnd {
                content_index,
                id,
                name,
                input_json,
            }),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{StreamDelta, TokenUsage};

    fn kinds(events: &[ProviderStreamEvent]) -> Vec<ProviderStreamEventKind> {
        events.iter().map(ProviderStreamEvent::kind).collect()
    }

    #[test]
    fn text_deltas_are_bracketed_by_lifecycle_events() {
        let mut assembler = ProviderStreamAssembler::new();
        let mut events = Vec::new();
        events.extend(assembler.push_delta(StreamDelta::TextDelta { text: "hi ".into() }));
        events.extend(assembler.push_delta(StreamDelta::TextDelta {
            text: "there".into(),
        }));
        events.extend(assembler.push_delta(StreamDelta::Done {
            finish_reason: Some("stop".into()),
            usage: Some(TokenUsage {
                input_tokens: 3,
                output_tokens: 2,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                reasoning_output_tokens: 0,
            }),
        }));

        assert_eq!(
            kinds(&events),
            vec![
                ProviderStreamEventKind::Start,
                ProviderStreamEventKind::TextStart,
                ProviderStreamEventKind::TextDelta,
                ProviderStreamEventKind::TextDelta,
                ProviderStreamEventKind::TextEnd,
                ProviderStreamEventKind::Done,
            ]
        );
        assert!(matches!(
            events[4],
            ProviderStreamEvent::TextEnd { ref content, .. } if content == "hi there"
        ));
    }

    #[test]
    fn thinking_closes_before_text_starts() {
        let mut assembler = ProviderStreamAssembler::new();
        let mut events = Vec::new();
        events.extend(assembler.push_delta(StreamDelta::ThinkingDelta {
            thinking: "plan".into(),
        }));
        events.extend(assembler.push_delta(StreamDelta::SignatureDelta {
            signature: "sig-1".into(),
        }));
        events.extend(assembler.push_delta(StreamDelta::TextDelta {
            text: "answer".into(),
        }));

        assert_eq!(
            kinds(&events),
            vec![
                ProviderStreamEventKind::Start,
                ProviderStreamEventKind::ThinkingStart,
                ProviderStreamEventKind::ThinkingDelta,
                ProviderStreamEventKind::ThinkingSignature,
                ProviderStreamEventKind::ThinkingEnd,
                ProviderStreamEventKind::TextStart,
                ProviderStreamEventKind::TextDelta,
            ]
        );
        assert!(matches!(
            events[4],
            ProviderStreamEvent::ThinkingEnd { ref content, .. } if content == "plan"
        ));
    }

    #[test]
    fn tool_call_deltas_are_bracketed_and_accumulated() {
        let mut assembler = ProviderStreamAssembler::new();
        let mut events = Vec::new();
        events.extend(assembler.push_delta(StreamDelta::ToolCallDelta {
            id: "tool-1".into(),
            name: Some("read_file".into()),
            input_json: None,
        }));
        events.extend(assembler.push_delta(StreamDelta::ToolCallDelta {
            id: "tool-1".into(),
            name: None,
            input_json: Some("{\"path\":".into()),
        }));
        events.extend(assembler.push_delta(StreamDelta::ToolCallDelta {
            id: "tool-1".into(),
            name: None,
            input_json: Some("\"a.txt\"}".into()),
        }));
        events.extend(assembler.finish());

        assert_eq!(
            kinds(&events),
            vec![
                ProviderStreamEventKind::Start,
                ProviderStreamEventKind::ToolCallStart,
                ProviderStreamEventKind::ToolCallDelta,
                ProviderStreamEventKind::ToolCallDelta,
                ProviderStreamEventKind::ToolCallEnd,
            ]
        );
        assert!(matches!(
            events[4],
            ProviderStreamEvent::ToolCallEnd { ref input_json, .. } if input_json == "{\"path\":\"a.txt\"}"
        ));
    }

    #[test]
    fn duplicate_done_is_ignored() {
        let mut assembler = ProviderStreamAssembler::new();
        let first = assembler.push_delta(StreamDelta::Done {
            finish_reason: Some("stop".into()),
            usage: None,
        });
        let second = assembler.push_delta(StreamDelta::Done {
            finish_reason: Some("stop".into()),
            usage: None,
        });

        assert_eq!(
            kinds(&first),
            vec![
                ProviderStreamEventKind::Start,
                ProviderStreamEventKind::Done
            ]
        );
        assert!(second.is_empty());
    }
}
