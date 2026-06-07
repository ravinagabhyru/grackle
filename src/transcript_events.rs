use serde::{Deserialize, Serialize};

use crate::ipc::OutputMode;

/// Window-visibility action requested of a subscribed UI (e.g. `grackle-ui`).
///
/// The daemon does not track UI visibility; `Toggle` is resolved UI-side.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UiAction {
    Show,
    Hide,
    Toggle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptEvent {
    Partial {
        seq: u64,
        text: String,
    },
    Final {
        seq: u64,
        raw_text: String,
        refined_text: String,
        output: OutputMode,
    },
    Error {
        message: String,
    },
    State {
        state: String,
        provider: String,
        model: String,
    },
    Ui {
        action: UiAction,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_round_trip(event: &TranscriptEvent, json: &str) {
        let serialized = serde_json::to_string(event).unwrap();
        assert_eq!(serialized, json);
        let deserialized: TranscriptEvent = serde_json::from_str(json).unwrap();
        assert_eq!(&deserialized, event);
    }

    #[test]
    fn transcript_event_serializes_exact_json_shapes() {
        assert_round_trip(
            &TranscriptEvent::Partial {
                seq: 4,
                text: "hello world".to_string(),
            },
            r#"{"type":"partial","seq":4,"text":"hello world"}"#,
        );
        assert_round_trip(
            &TranscriptEvent::Final {
                seq: 4,
                raw_text: "hello world".to_string(),
                refined_text: "Hello, world.".to_string(),
                output: OutputMode::Wtype,
            },
            r#"{"type":"final","seq":4,"raw_text":"hello world","refined_text":"Hello, world.","output":"wtype"}"#,
        );
        assert_round_trip(
            &TranscriptEvent::Error {
                message: "sink failed".to_string(),
            },
            r#"{"type":"error","message":"sink failed"}"#,
        );
        assert_round_trip(
            &TranscriptEvent::State {
                state: "ContinuousRunning".to_string(),
                provider: "Parakeet".to_string(),
                model: "parakeet-nemotron".to_string(),
            },
            r#"{"type":"state","state":"ContinuousRunning","provider":"Parakeet","model":"parakeet-nemotron"}"#,
        );
    }

    #[test]
    fn transcript_event_ui_serializes_exact_json_shapes() {
        assert_round_trip(
            &TranscriptEvent::Ui {
                action: UiAction::Show,
            },
            r#"{"type":"ui","action":"show"}"#,
        );
        assert_round_trip(
            &TranscriptEvent::Ui {
                action: UiAction::Hide,
            },
            r#"{"type":"ui","action":"hide"}"#,
        );
        assert_round_trip(
            &TranscriptEvent::Ui {
                action: UiAction::Toggle,
            },
            r#"{"type":"ui","action":"toggle"}"#,
        );
    }
}
