//! Non-destructive mask inspection state shared by command and canvas layers.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MaskInspection {
    target: Option<String>,
}

impl MaskInspection {
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub fn active(&self) -> bool {
        self.target.is_some()
    }

    pub fn enter(&mut self, layer_id: impl Into<String>) {
        self.target = Some(layer_id.into());
    }

    pub fn exit(&mut self) {
        self.target = None;
    }

    pub fn toggle(&mut self, layer_id: impl Into<String>) {
        let layer_id = layer_id.into();
        if self.target.as_deref() == Some(layer_id.as_str()) {
            self.exit();
        } else {
            self.enter(layer_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_is_reversible_and_does_not_change_target_identity() {
        let mut view = MaskInspection::default();
        view.enter("layer");
        assert_eq!(view.target(), Some("layer"));
        view.toggle("other");
        assert_eq!(view.target(), Some("other"));
        view.toggle("other");
        assert!(!view.active());
    }
}
