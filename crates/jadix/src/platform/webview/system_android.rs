//! NativeActivity-compatible android.webkit.WebView bridge. All view mutations
//! are dispatched by JadixWebViewHost onto Activity.runOnUiThread.
use super::super::{WebViewSpec, WebViewStatus};
use jni::{
    objects::{GlobalRef, JClass, JObject, JString, JValue},
    JNIEnv, JavaVM,
};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
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
    scope: crate::app::window_scope::WindowScope,
    generation: Cell<u64>,
    evaluation_serial: Cell<i64>,
    evaluations: RefCell<HashMap<i64, jadix_core::web_evaluation::EvaluationReply>>,
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
            scope: crate::app::window_scope::active(),
            generation: Cell::new(0),
            evaluation_serial: Cell::new(0),
            evaluations: RefCell::default(),
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
                "documentState",
                "(Landroid/app/Activity;Ljava/lang/String;)J",
                &[
                    JValue::Object(self.activity.as_obj()),
                    JValue::Object(id.as_ref()),
                ],
            )
            .and_then(|v| v.j())
        });
        if let Ok(state) = result {
            let generation = (state as u64) >> 2;
            if self.generation.replace(generation) != generation {
                super::super::document_changed(self.scope, &self.id);
            }
        }
        self.evaluations.borrow_mut().retain(|serial, reply| {
            let active = reply.is_active();
            let raw = self.with_env(|env| {
                let id = env.new_string(&self.id)?;
                let class: &JClass = self.class.as_obj().into();
                let value = env
                    .call_static_method(
                        class,
                        "takeEvaluation",
                        "(Landroid/app/Activity;Ljava/lang/String;JZ)Ljava/lang/String;",
                        &[
                            JValue::Object(self.activity.as_obj()),
                            JValue::Object(id.as_ref()),
                            JValue::Long(*serial),
                            JValue::Bool((!active) as u8),
                        ],
                    )?
                    .l()?;
                if value.is_null() {
                    Ok(None)
                } else {
                    Ok(Some(env.get_string(&JString::from(value))?.into()))
                }
            });
            match raw {
                Ok(Some(raw)) => {
                    reply.complete(Ok(raw));
                    false
                }
                Err(error) => {
                    reply.complete(Err(error));
                    false
                }
                Ok(None) => active,
            }
        });
        match result {
            Ok(n) if n & 3 == 1 => Some(WebViewStatus::Ready),
            Ok(n) if n & 3 == 2 => Some(WebViewStatus::Failed(
                "Android WebView navigation failed".into(),
            )),
            Ok(_) => Some(WebViewStatus::Loading),
            Err(_) => None,
        }
    }
    pub fn evaluate(
        &self,
        script: &str,
        reply: jadix_core::web_evaluation::EvaluationReply,
    ) -> Result<(), String> {
        let serial = if reply.is_active() {
            let serial = self
                .evaluation_serial
                .get()
                .checked_add(1)
                .ok_or("Evaluation IDs exhausted")?;
            self.evaluation_serial.set(serial);
            serial
        } else {
            0
        };
        self.with_env(|env| {
            let id = env.new_string(&self.id)?;
            let script = env.new_string(script)?;
            let class: &JClass = self.class.as_obj().into();
            env.call_static_method(
                class,
                "evaluate",
                "(Landroid/app/Activity;Ljava/lang/String;JLjava/lang/String;)V",
                &[
                    JValue::Object(self.activity.as_obj()),
                    JValue::Object(id.as_ref()),
                    JValue::Long(serial),
                    JValue::Object(script.as_ref()),
                ],
            )?;
            Ok(())
        })?;
        if serial != 0 {
            self.evaluations.borrow_mut().insert(serial, reply);
        }
        Ok(())
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
