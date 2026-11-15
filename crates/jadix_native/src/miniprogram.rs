use crate::{
    host::{ApplyReceipt, PlatformHost},
    layout::{LayoutError, MeasureRequest},
    presentation::{
        capabilities::BackendCapabilities, CommitBatch, MeasureResult, NodeDescriptor, NodeId,
        PresentationNode, RootId,
    },
    wire,
};

type Measure = dyn FnMut(&str) -> Result<[f64; 2], String>;
type Publish = dyn FnMut(&str) -> Result<(), String>;

pub struct MiniProgramHost {
    measure: Box<Measure>,
    publish: Box<Publish>,
    dispose: Box<dyn FnMut()>,
    alive: bool,
    accepted: Option<(RootId, u64)>,
}

impl MiniProgramHost {
    pub fn new(
        measure: impl FnMut(&str) -> Result<[f64; 2], String> + 'static,
        publish: impl FnMut(&str) -> Result<(), String> + 'static,
        dispose: impl FnMut() + 'static,
    ) -> Self {
        Self {
            measure: Box::new(measure),
            publish: Box::new(publish),
            dispose: Box::new(dispose),
            alive: true,
            accepted: None,
        }
    }
}

impl PlatformHost for MiniProgramHost {
    fn is_alive(&self) -> bool {
        self.alive
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::headless()
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        if !self.alive {
            return Err(LayoutError::Disposed);
        }
        let size = (self.measure)(&wire::measurement(request, descriptor))
            .map_err(|_| LayoutError::MeasurementUnavailable(request.key.node))?;
        if size.iter().any(|value| !value.is_finite() || *value < 0.0) {
            return Err(LayoutError::InvalidMeasurement(request.key.node));
        }
        Ok(MeasureResult {
            key: request.key,
            width: size[0],
            height: size[1],
            baseline: None,
        })
    }

    fn publish(&mut self, _: RootId, _: u64, _: &[PresentationNode]) -> Result<(), String> {
        Err("miniprogram requires revisioned protocol-2 commits".into())
    }

    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        _: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        if !self.alive {
            return Err("miniprogram host disposed".into());
        }
        (self.publish)(&wire::batch(batch))?;
        self.accepted = Some((batch.root, batch.revision));
        Ok(ApplyReceipt::for_batch(batch))
    }

    fn scroll_to(&mut self, node: NodeId, offset: [f64; 2]) -> Result<(), String> {
        let (root, revision) = self
            .accepted
            .filter(|(root, _)| *root == node.root)
            .ok_or("miniprogram root is not mounted")?;
        if !self.alive
            || offset
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err("invalid miniprogram scroll request".into());
        }
        (self.publish)(
            &serde_json::json!({"command": "scroll", "root": root.token(),
            "revision": revision.to_string(), "node": node.token(), "offset": offset})
            .to_string(),
        )
    }

    fn dispose(&mut self) {
        if self.alive {
            self.alive = false;
            self.accepted = None;
            (self.dispose)();
        }
    }
}

impl Drop for MiniProgramHost {
    fn drop(&mut self) {
        self.dispose();
    }
}
