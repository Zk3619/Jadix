# jadix_net

## Native HTTPS transport

The optional `native-https` feature provides `native_https::HttpsClient`. The host
selects an installed curl executable and runs `get` on its own worker. TLS uses
that transport's trust store, requires TLS 1.2 or newer, ignores curlrc, verifies
the hostname, and forbids redirects and non-HTTPS protocols. An explicit
`with_ca_bundle` supports private PKI without changing system trust.

Requests carry a deadline, cancellation token and decoded-body budget (at most
32 MiB); headers have a separate 64 KiB ceiling. Read chunks are bounded and the
owned curl process is killed on cancellation, timeout or oversized output. This
adapter accepts GET only and does not install curl or an executor. Feature-free
networking contracts remain available independently. `examples/https_get.rs`
exercises the same adapter against a host-selected HTTPS URL.

[English](README.md) | [简体中文](README.zh.md)

Backend-neutral networking contracts for Jadix and external adapters.

Part of [Jadix](https://github.com/Zk3619/Jadix). **Alpha; version `0.1.0`.**

## Installation

Add the package to your application:

```toml
[dependencies]
jadix_net = "=0.1.0"
```

Platform-specific packages require the corresponding SDK.

## Features and scope

Default features: none. See [Cargo.toml](Cargo.toml) for the complete feature/dependency graph. A feature flag does not establish platform runtime validation. Applications normally select internal GPU/window packages through their facade rather than depending on platform internals.

## Usage details

The existing [package guide](GUIDE.md) retains additional API, platform and source details; it may use another language.

## Documentation

[Project documentation](https://github.com/Zk3619/Jadix/tree/main/docs) covers application setup, platform integration, configuration, and API usage.

## License

Apache-2.0. Preserve [LICENSE](LICENSE) and dependency notices.
