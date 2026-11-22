//! Private stdio bridge for `scripts/native.py update-*`; secrets never enter argv.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use jadix_core::updater::{self, Config};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, Read};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Key { seed: String },
    Config { config: Config },
    Release { seed: Option<String>, manifest_url: String, manifest: updater::Manifest },
}
fn seed(value: &str) -> Result<[u8; 32], String> {
    B64.decode(value.trim()).map_err(|_| "invalid signing seed")?.try_into().map_err(|_| "signing seed must be 32 bytes".into())
}
fn run() -> Result<serde_json::Value, String> {
    let mut input = Vec::new();
    io::stdin().take((updater::MANIFEST_LIMIT + 65537) as u64).read_to_end(&mut input).map_err(|_| "cannot read updater request")?;
    if input.len() > updater::MANIFEST_LIMIT + 65536 { return Err("updater request exceeds limit".into()); }
    match serde_json::from_slice::<Request>(&input).map_err(|_| "invalid updater tool request")? {
        Request::Key { seed: value } => {
            let key = ed25519_dalek::SigningKey::from_bytes(&seed(&value)?);
            Ok(json!({"publicKey": B64.encode(key.verifying_key().as_bytes())}))
        }
        Request::Config { config } => { config.validate()?; Ok(json!({"config": config})) }
        Request::Release { seed: value, manifest_url, manifest } => {
            let bytes = serde_json::to_vec(&manifest).map_err(|_| "cannot serialize manifest")?;
            updater::validate_release(&manifest_url, &bytes)?;
            let (public_key, signature) = match value {
                Some(value) => { let (key, signature) = updater::sign_release(&seed(&value)?, &manifest_url, &bytes)?; (Some(key), Some(signature)) },
                None => (None, None),
            };
            Ok(json!({"manifest": String::from_utf8(bytes).unwrap(), "signature": signature, "publicKey": public_key}))
        }
    }
}
fn main() {
    match run() {
        Ok(value) => println!("{value}"),
        Err(error) => { eprintln!("{error}"); std::process::exit(1); }
    }
}
