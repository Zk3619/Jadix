use super::PlatformApp as NativeApp;
use jadix_native::android::AndroidHost;
use jni::{
    objects::{JClass, JObject, JString},
    sys::{jfloat, jlong, jstring},
    JNIEnv,
};
use std::{cell::RefCell, collections::BTreeMap};

thread_local! {
    #[allow(clippy::missing_const_for_thread_local, reason = "Android TLS fallback is linted inside the macro despite its const initializer")]
    static APPS: RefCell<BTreeMap<String, NativeApp<AndroidHost>>> = const { RefCell::new(BTreeMap::new()) };
}

fn output(env: &mut JNIEnv<'_>, result: Result<String, String>) -> jstring {
    match result {
        Ok(value) => env
            .new_string(value)
            .map_or(std::ptr::null_mut(), |value| value.into_raw()),
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error);
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_jadix_nativeui_NativeView_nativeCreate(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    peer: JObject<'_>,
    width: jfloat,
    height: jfloat,
    count: jlong,
    text: JString<'_>,
) -> jstring {
    let result = (|| {
        let text: String = env
            .get_string(&text)
            .map_err(|error| error.to_string())?
            .into();
        let host = AndroidHost::new(&env, &peer).map_err(|error| error.to_string())?;
        let app = NativeApp::new(
            host,
            [width, height],
            usize::try_from(count).map_err(|error| error.to_string())?,
            text,
        )?;
        let token = app.root.model().id().token();
        APPS.with(|apps| apps.borrow_mut().insert(token.clone(), app));
        Ok(token)
    })();
    output(&mut env, result)
}

fn with_app(
    env: &mut JNIEnv<'_>,
    token: JString<'_>,
    action: impl FnOnce(&mut NativeApp<AndroidHost>) -> Result<String, String>,
) -> jstring {
    let result = (|| {
        let token: String = env
            .get_string(&token)
            .map_err(|error| error.to_string())?
            .into();
        APPS.with(|apps| {
            let mut apps = apps
                .try_borrow_mut()
                .map_err(|_| String::from("reentrant native event"))?;
            action(
                apps.get_mut(&token)
                    .ok_or_else(|| String::from("disposed native root"))?,
            )
        })
    })();
    output(env, result)
}

#[cfg(any(feature = "notes", feature = "field-workflow"))]
#[no_mangle]
pub extern "system" fn Java_dev_jadix_nativeui_NotesActivity_nativeStorageResult(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    token: JString<'_>,
    error: JString<'_>,
) -> jstring {
    let error = if error.is_null() {
        None
    } else {
        match env.get_string(&error) {
            Ok(value) => Some(String::from(value)),
            Err(error) => return output(&mut env, Err(error.to_string())),
        }
    };
    with_app(&mut env, token, |app| {
        app.storage_result(error)?;
        Ok(app.state())
    })
}

#[cfg(feature = "field-workflow")]
#[no_mangle]
pub extern "system" fn Java_dev_jadix_nativeui_FieldActivity_nativeStorageResult(
    env: JNIEnv<'_>,
    class: JClass<'_>,
    token: JString<'_>,
    error: JString<'_>,
) -> jstring {
    Java_dev_jadix_nativeui_NotesActivity_nativeStorageResult(env, class, token, error)
}

#[no_mangle]
pub extern "system" fn Java_dev_jadix_nativeui_NativeView_nativeEvent(
    env: JNIEnv<'_>,
    _: JClass<'_>,
    token: JString<'_>,
    json: JString<'_>,
) -> jstring {
    event(env, token, json, false)
}

#[no_mangle]
pub extern "system" fn Java_dev_jadix_nativeui_NativeView_nativeDispatch(
    env: JNIEnv<'_>,
    _: JClass<'_>,
    token: JString<'_>,
    json: JString<'_>,
) -> jstring {
    event(env, token, json, true)
}

fn event(
    mut env: JNIEnv<'_>,
    token: JString<'_>,
    json: JString<'_>,
    notification: bool,
) -> jstring {
    let json = match env.get_string(&json) {
        Ok(value) => String::from(value),
        Err(error) => return output(&mut env, Err(error.to_string())),
    };
    with_app(&mut env, token, |app| {
        app.event(&json)?;
        if notification {
            #[cfg(feature = "notes")]
            return Ok(app.storage_notification().unwrap_or_default());
        }
        Ok(app.state())
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jadix_nativeui_NativeView_nativeResize(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    token: JString<'_>,
    width: jfloat,
    height: jfloat,
    environment: jlong,
) -> jstring {
    with_app(&mut env, token, |app| {
        app.resize([width, height], environment as u64)?;
        Ok(app.state())
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jadix_nativeui_NativeView_nativeState(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    token: JString<'_>,
) -> jstring {
    with_app(&mut env, token, |app| Ok(app.state()))
}

#[no_mangle]
pub extern "system" fn Java_dev_jadix_nativeui_NativeView_nativeRestore(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    token: JString<'_>,
    json: JString<'_>,
) -> jstring {
    let json = match env.get_string(&json) {
        Ok(value) => String::from(value),
        Err(error) => return output(&mut env, Err(error.to_string())),
    };
    with_app(&mut env, token, |app| {
        app.restore(&json)?;
        Ok(app.state())
    })
}

#[no_mangle]
pub extern "system" fn Java_dev_jadix_nativeui_NativeView_nativeDispose(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    token: JString<'_>,
) {
    let result = env
        .get_string(&token)
        .map(String::from)
        .map_err(|error| error.to_string());
    match result {
        Ok(token) => {
            APPS.with(|apps| {
                apps.borrow_mut().remove(&token);
            });
        }
        Err(error) => {
            let _ = env.throw_new("java/lang/IllegalStateException", error);
        }
    }
}
