#[derive(Clone)]
pub(crate) struct Draft {
    pub(crate) value: String,
    pub(crate) revision: u64,
    preview: Option<String>,
    pub(crate) committed: bool,
}

impl Draft {
    pub(crate) fn new(value: String) -> Self {
        Self {
            value,
            revision: 0,
            preview: None,
            committed: false,
        }
    }

    pub(crate) fn display(&self) -> &str {
        self.preview.as_deref().unwrap_or(&self.value)
    }

    pub(crate) fn is_composing(&self) -> bool {
        self.preview.is_some()
    }

    pub(crate) fn update(&mut self, value: &str, revision: u64, composing: bool) {
        self.revision = revision;
        if composing {
            self.preview = Some(value.into());
        } else {
            self.value = value.into();
            self.preview = None;
            self.committed = true;
        }
    }
}
