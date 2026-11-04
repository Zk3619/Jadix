use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JFloatArray, JObject, JString, JValue},
};

use crate::host::{ApplyReceipt, PlatformHost};
use crate::layout::{LayoutError, MeasureRequest};
use crate::presentation::{CommitBatch, MeasureResult, NodeDescriptor, PresentationNode, RootId};

pub struct AndroidHost {
    vm: JavaVM,
    peer: GlobalRef,
}

impl AndroidHost {
    pub fn new(env: &JNIEnv<'_>, peer: &JObject<'_>) -> jni::errors::Result<Self> {
        Ok(Self {
            vm: env.get_java_vm()?,
            peer: env.new_global_ref(peer)?,
        })
    }

    fn call(&self, method: &str, signature: &str, json: &str) -> Result<Option<[f32; 2]>, String> {
        let mut env = self.vm.get_env().map_err(|error| error.to_string())?;
        let value = env.new_string(json).map_err(|error| error.to_string())?;
        let result = env.call_method(
            self.peer.as_obj(),
            method,
            signature,
            &[JValue::Object(value.as_ref())],
        );
        if env.exception_check().unwrap_or(true) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            return Err(format!("Android host {method} failed"));
        }
        let result = result.map_err(|error| error.to_string())?;
        if method == "measure" {
            let array = JFloatArray::from(result.l().map_err(|error| error.to_string())?);
            if env
                .get_array_length(&array)
                .map_err(|error| error.to_string())?
                != 2
            {
                return Err(String::from("invalid Android measurement"));
            }
            let mut size = [0.0; 2];
            env.get_float_array_region(&array, 0, &mut size)
                .map_err(|error| error.to_string())?;
            Ok(Some(size))
        } else {
            Ok(None)
        }
    }
}

impl PlatformHost for AndroidHost {
    fn capabilities(&self) -> crate::presentation::capabilities::BackendCapabilities {
        crate::presentation::capabilities::BackendCapabilities::headless()
    }

    fn measure(
        &mut self,
        request: MeasureRequest,
        descriptor: &NodeDescriptor,
    ) -> Result<MeasureResult, LayoutError> {
        let [width, height] = self
            .call(
                "measure",
                "(Ljava/lang/String;)[F",
                &crate::wire::measurement(request, descriptor),
            )
            .ok()
            .flatten()
            .ok_or(LayoutError::MeasurementUnavailable(request.key.node))?;
        if !width.is_finite() || !height.is_finite() || width < 0.0 || height < 0.0 {
            return Err(LayoutError::InvalidMeasurement(request.key.node));
        }
        Ok(MeasureResult {
            key: request.key,
            width: f64::from(width).min(request.key.max_width),
            height: f64::from(height).min(request.key.max_height),
            baseline: None,
        })
    }

    fn publish(
        &mut self,
        root: RootId,
        revision: u64,
        roots: &[PresentationNode],
    ) -> Result<(), String> {
        self.call(
            "publish",
            "(Ljava/lang/String;)V",
            &crate::wire::snapshot(root, revision, roots),
        )
        .map(|_| ())
    }

    fn publish_batch(
        &mut self,
        batch: &CommitBatch,
        _: &[PresentationNode],
    ) -> Result<ApplyReceipt, String> {
        let mut env = self.vm.get_env().map_err(|error| error.to_string())?;
        let value = env
            .new_string(crate::wire::batch(batch))
            .map_err(|error| error.to_string())?;
        let result = env.call_method(
            self.peer.as_obj(),
            "publishBatch",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[JValue::Object(value.as_ref())],
        );
        if env.exception_check().unwrap_or(true) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            return Err(String::from("Android native batch failed"));
        }
        let receipt = JString::from(
            result
                .map_err(|error| error.to_string())?
                .l()
                .map_err(|error| error.to_string())?,
        );
        let receipt: String = env
            .get_string(&receipt)
            .map_err(|error| error.to_string())?
            .into();
        if receipt
            != format!(
                "{}:{}:{}",
                batch.root.token(),
                batch.previous,
                batch.revision
            )
        {
            return Err(String::from(
                "Android native batch acknowledgement mismatch",
            ));
        }
        Ok(ApplyReceipt::for_batch(batch))
    }

    fn dispose(&mut self) {
        let _ = self.call("clear", "(Ljava/lang/String;)V", "{}");
    }
}
