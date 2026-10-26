//! NativeActivity-compatible android.webkit.WebView bridge. All view mutations
//! are dispatched by JadixWebViewHost onto Activity.runOnUiThread.
use super::super::{WebViewSpec, WebViewStatus};
use jni::{
    objects::{GlobalRef, JClass, JObject, JValue},
    JNIEnv, JavaVM,
};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};
use winit::window::Window;

pub fn initialize() -> Result<(), String> {
    Ok(())
}
pub fn pump() {}
pub struct Browser {
    vm: JavaVM,
    activity: GlobalRef,
    class: GlobalRef,
    id: String,
    previous: RefCell<Option<WebViewSpec>>,
    scale: Cell<f32>,
    visibility: Cell<Option<bool>>,
}
impl Browser {
    pub fn new(_: &Arc<dyn Window>, spec: &WebViewSpec, scale: f32) -> Result<Self, String> {
        let app = crate::platform::android_app();
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }.map_err(|e| e.to_string())?;
        let (activity, class) = {
            let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
            env.with_local_frame(16, |env| {
                let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
                let activity = env.new_global_ref(activity)?;
                let loader = env
                    .call_method(
                        activity.as_obj(),
                        "getClassLoader",
                        "()Ljava/lang/ClassLoader;",
                        &[],
                    )
                    .and_then(|v| v.l())?;
                let name = env.new_string("org.jadix.webview.JadixWebViewHost")?;
                let class = env
                    .call_method(
                        loader,
                        "loadClass",
                        "(Ljava/lang/String;)Ljava/lang/Class;",
                        &[JValue::Object(name.as_ref())],
                    )
                    .and_then(|v| v.l());
                Ok::<_, jni::errors::Error>((activity, env.new_global_ref(class?)?))
            })
            .map_err(|e| {
                let _ = env.exception_clear();
                format!("Add JadixWebViewHost.java to the Android application source set: {e}")
            })?
        };
        let result = Self {
            vm,
            activity,
            class,
            id: spec.id.clone(),
            previous: RefCell::new(None),
            scale: Cell::new(scale),
            visibility: Cell::new(None),
        };
        result.update(spec, scale)?;
        Ok(result)
    }
    fn with_env<T>(
        &self,
        call: impl FnOnce(&mut JNIEnv<'_>) -> jni::errors::Result<T>,
    ) -> Result<T, String> {
        let mut env = self.vm.attach_current_thread().map_err(|e| e.to_string())?;
        env.with_local_frame(16, call).map_err(|e| {
            let _ = env.exception_clear();
            e.to_string()
        })
    }
    pub fn update(&self, spec: &WebViewSpec, scale: f32) -> Result<(), String> {
        if self.previous.borrow().as_ref() == Some(spec) && self.scale.get() == scale {
            return Ok(());
        }
        self.with_env(|env| {
        let class: &JClass = self.class.as_obj().into();
        let id = env.new_string(&spec.id)?;
        let url = env.new_string(&spec.url)?;
        let html = spec.html.as_ref().map(|s| env.new_string(s)).transpose()?;
        let null = JObject::null();
        let html = html.as_ref().map(|s| s.as_ref()).unwrap_or(&null);
        let b = spec.bounds.map(|v| (v * scale).round() as i32);
        let c = spec.clip.map(|v| (v * scale).round() as i32);
        env.call_static_method(class, "sync", "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;IIIIIIIIZ)V", &[
            JValue::Object(self.activity.as_obj()), JValue::Object(id.as_ref()), JValue::Object(url.as_ref()), JValue::Object(html),
            JValue::Int(b[0]), JValue::Int(b[1]), JValue::Int(b[2]), JValue::Int(b[3]), JValue::Int(c[0]), JValue::Int(c[1]), JValue::Int(c[2]), JValue::Int(c[3]), JValue::Bool(spec.visible as u8)
        ])?;
        Ok(())
        })?;
        self.visibility.set(Some(spec.visible));
        *self.previous.borrow_mut() = Some(spec.clone());
        self.scale.set(scale);
        Ok(())
    }
    pub fn visible(&self, visible: bool) {
        if self.visibility.get() == Some(visible) {
            return;
        }
        if self
            .with_env(|env| {
                let id = env.new_string(&self.id)?;
                let class: &JClass = self.class.as_obj().into();
                env.call_static_method(
                    class,
                    "visible",
                    "(Landroid/app/Activity;Ljava/lang/String;Z)V",
                    &[
                        JValue::Object(self.activity.as_obj()),
                        JValue::Object(id.as_ref()),
                        JValue::Bool(visible as u8),
                    ],
                )?;
                Ok(())
            })
            .is_ok()
        {
            self.visibility.set(Some(visible));
        }
    }
    pub fn poll(&self) -> Option<WebViewStatus> {
        let result = self.with_env(|env| {
            let id = env.new_string(&self.id)?;
            let class: &JClass = self.class.as_obj().into();
            env.call_static_method(
                class,
                "status",
                "(Landroid/app/Activity;Ljava/lang/String;)I",
                &[
                    JValue::Object(self.activity.as_obj()),
                    JValue::Object(id.as_ref()),
                ],
            )
            .and_then(|v| v.i())
        });
        match result {
            Ok(1) => Some(WebViewStatus::Ready),
            Ok(n) if n < 0 => Some(WebViewStatus::Failed(
                "Android WebView navigation failed".into(),
            )),
            Ok(_) => Some(WebViewStatus::Loading),
            Err(_) => None,
        }
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.with_env(|env| {
            let id = env.new_string(&self.id)?;
            let class: &JClass = self.class.as_obj().into();
            env.call_static_method(
                class,
                "remove",
                "(Landroid/app/Activity;Ljava/lang/String;)V",
                &[
                    JValue::Object(self.activity.as_obj()),
                    JValue::Object(id.as_ref()),
                ],
            )?;
            Ok(())
        });
    }
}
