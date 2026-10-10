use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypingTier {
    /// Tier 1: Direct CDP Input.insertText / dispatchKeyEvent
    CdpInsertText,
    /// Tier 2: document.execCommand('insertText') via DOM
    DomExecCommand,
    /// Tier 3: Native HTML input/textarea value property setter
    NativeSetter,
    /// Tier 4: Character-by-character key events
    CharKeyEvents,
    /// Tier 5: Final readback verification
    VerifyValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingAction {
    pub backend_dom_node_id: u64,
    pub text: String,
    pub submit: bool,
    pub fallback_tier: TypingTier,
}

impl TypingAction {
    pub fn new(backend_dom_node_id: u64, text: impl Into<String>, submit: bool) -> Self {
        Self {
            backend_dom_node_id,
            text: text.into(),
            submit,
            fallback_tier: TypingTier::CdpInsertText,
        }
    }
}
