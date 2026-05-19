use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualPerceptionProviderKind {
    PaddleOcr,
    EasyOcr,
    VlmGrounding,
    Mock,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrTextBox {
    pub text: String,
    pub confidence: f64,
    pub r#box: VisualBox,
    pub source: VisualPerceptionProviderKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualControlCandidate {
    pub label: String,
    pub role: Option<String>,
    pub confidence: f64,
    pub r#box: VisualBox,
    pub source: VisualPerceptionProviderKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualObservation {
    pub screenshot_ref: String,
    pub provider: VisualPerceptionProviderKind,
    pub ocr_text: Vec<OcrTextBox>,
    pub detected_controls: Vec<VisualControlCandidate>,
}

#[async_trait]
pub trait VisualPerceptionProvider: Send + Sync {
    fn kind(&self) -> VisualPerceptionProviderKind;

    async fn analyze_screenshot(
        &self,
        screenshot_ref: &str,
        screenshot_b64: &str,
    ) -> Result<Option<VisualObservation>>;
}

#[derive(Debug, Default)]
pub struct NoopVisualPerceptionProvider;

#[async_trait]
impl VisualPerceptionProvider for NoopVisualPerceptionProvider {
    fn kind(&self) -> VisualPerceptionProviderKind {
        VisualPerceptionProviderKind::Noop
    }

    async fn analyze_screenshot(
        &self,
        _screenshot_ref: &str,
        _screenshot_b64: &str,
    ) -> Result<Option<VisualObservation>> {
        Ok(None)
    }
}

#[derive(Debug, Clone)]
pub struct MockVisualPerceptionProvider {
    observation: Option<VisualObservation>,
}

impl MockVisualPerceptionProvider {
    pub fn new(observation: Option<VisualObservation>) -> Self {
        Self { observation }
    }
}

#[async_trait]
impl VisualPerceptionProvider for MockVisualPerceptionProvider {
    fn kind(&self) -> VisualPerceptionProviderKind {
        VisualPerceptionProviderKind::Mock
    }

    async fn analyze_screenshot(
        &self,
        screenshot_ref: &str,
        _screenshot_b64: &str,
    ) -> Result<Option<VisualObservation>> {
        Ok(self.observation.clone().map(|mut observation| {
            observation.screenshot_ref = screenshot_ref.to_string();
            observation.provider = VisualPerceptionProviderKind::Mock;
            observation
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_box() -> VisualBox {
        VisualBox {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 30.0,
        }
    }

    #[tokio::test]
    async fn noop_provider_degrades_to_no_visual_observation() {
        let provider = NoopVisualPerceptionProvider;
        let got = provider
            .analyze_screenshot("screenshot://run/1", "base64-png")
            .await
            .unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn mock_provider_returns_text_boxes_with_screenshot_ref() {
        let provider = MockVisualPerceptionProvider::new(Some(VisualObservation {
            screenshot_ref: String::new(),
            provider: VisualPerceptionProviderKind::Noop,
            ocr_text: vec![OcrTextBox {
                text: "visual-only button".into(),
                confidence: 0.98,
                r#box: sample_box(),
                source: VisualPerceptionProviderKind::Mock,
            }],
            detected_controls: vec![],
        }));

        let got = provider
            .analyze_screenshot("screenshot://session/tab", "base64-png")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(got.screenshot_ref, "screenshot://session/tab");
        assert_eq!(got.provider, VisualPerceptionProviderKind::Mock);
        assert_eq!(got.ocr_text[0].text, "visual-only button");
    }

    #[test]
    fn visual_observation_serializes_camelcase() {
        let observation = VisualObservation {
            screenshot_ref: "screenshot://session/tab".into(),
            provider: VisualPerceptionProviderKind::EasyOcr,
            ocr_text: vec![OcrTextBox {
                text: "Login".into(),
                confidence: 0.91,
                r#box: sample_box(),
                source: VisualPerceptionProviderKind::EasyOcr,
            }],
            detected_controls: vec![VisualControlCandidate {
                label: "Login".into(),
                role: Some("button".into()),
                confidence: 0.88,
                r#box: sample_box(),
                source: VisualPerceptionProviderKind::EasyOcr,
            }],
        };

        let json = serde_json::to_string(&observation).unwrap();
        assert!(
            json.contains("\"screenshotRef\":\"screenshot://session/tab\""),
            "{json}"
        );
        assert!(json.contains("\"ocrText\""), "{json}");
        assert!(json.contains("\"detectedControls\""), "{json}");
        assert!(json.contains("\"easy_ocr\""), "{json}");
    }
}
