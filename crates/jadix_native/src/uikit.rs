use crate::{
    host::PlatformHost,
    layout::{LayoutError, MeasureRequest},
    presentation::*,
};
use std::{
    ffi::{CString, c_char, c_void},
    marker::PhantomData,
    rc::Rc,
};

#[derive(Clone, Copy)]
pub struct UIKitCallbacks {
    pub measure: unsafe extern "C" fn(*mut c_void, *const c_char, *mut f64) -> bool,
    pub publish: unsafe extern "C" fn(*mut c_void, *const c_char) -> bool,
    pub clear: unsafe extern "C" fn(*mut c_void),
}

pub struct UIKitHost {
    context: *mut c_void,
    callbacks: UIKitCallbacks,
    _thread: PhantomData<Rc<()>>,
}

impl UIKitHost {
    /// # Safety
    /// The context and callbacks must remain valid on the current UI thread until
    /// this host is dropped. Callbacks must not unwind or retain borrowed buffers.
    /// Measurement writes exactly two finite nonnegative doubles to its output.
    pub unsafe fn new(context: *mut c_void, callbacks: UIKitCallbacks) -> Self {
        Self {
            context,
            callbacks,
            _thread: PhantomData,
        }
    }
}

impl PlatformHost for UIKitHost {
    fn capabilities(&self) -> capabilities::BackendCapabilities {
        capabilities::BackendCapabilities::headless()
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        let json = CString::new(crate::wire::measurement(request, descriptor))
            .map_err(|_| LayoutError::MeasurementUnavailable(request.key.node))?;
        let mut size = [f64::NAN; 2];
        if !unsafe { (self.callbacks.measure)(self.context, json.as_ptr(), size.as_mut_ptr()) } {
            return Err(LayoutError::MeasurementUnavailable(request.key.node));
        }
        if size.iter().any(|value| !value.is_finite() || *value < 0.0) {
            return Err(LayoutError::InvalidMeasurement(request.key.node));
        }
        Ok(MeasureResult {
            key: request.key,
            width: size[0].min(request.key.max_width),
            height: size[1].min(request.key.max_height),
            baseline: None,
        })
    }
    fn publish(
        &mut self,
        root: RootId,
        revision: u64,
        roots: &[PresentationNode],
    ) -> Result<(), String> {
        let json = CString::new(crate::wire::snapshot(root, revision, roots))
            .map_err(|error| error.to_string())?;
        if unsafe { (self.callbacks.publish)(self.context, json.as_ptr()) } {
            Ok(())
        } else {
            Err("UIKit rejected native commit".into())
        }
    }
    fn dispose(&mut self) {
        unsafe { (self.callbacks.clear)(self.context) };
    }
}
