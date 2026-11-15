//! Explicit installed-curl HTTPS transport probe (no global TLS configuration).
#[cfg(all(feature = "native-https", not(target_arch = "wasm32")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use jadix_net::{native_https::HttpsClient, CancellationToken, HttpHeader, HttpRequest};
    let mut arguments = std::env::args().skip(1);
    let executable = arguments.next().ok_or("curl executable required")?;
    let url = arguments.next().ok_or("HTTPS URL required")?;
    let mut request = HttpRequest::get(url).max_response_bytes(256 * 1024);
    request.headers.push(HttpHeader::new(
        "User-Agent",
        b"Jadix-HTTPS-acceptance".to_vec(),
    ));
    let start = std::time::Instant::now();
    let response = HttpsClient::new(executable).get(request, &CancellationToken::new())?;
    println!(
        "{{\"status\":{},\"body_bytes\":{},\"elapsed_ms\":{},\"private_ca_override\":false}}",
        response.status,
        response.body.len(),
        start.elapsed().as_millis()
    );
    if response.status != 200 {
        return Err("non-success HTTPS status".into());
    }
    Ok(())
}
#[cfg(any(not(feature = "native-https"), target_arch = "wasm32"))]
fn main() {
    eprintln!("Enable native-https on a native host");
}
