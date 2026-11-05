//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The one place an HTTP agent here gets the roots it trusts.
//!
//! The roots are the system's, and the ones `SSL_CERT_FILE` or `SSL_CERT_DIR` name where set,
//! which is how a cloud container's proxy authority reaches the registry indexes and the forges.
//! They are read once for the process, and every agent starts from [`agent_builder`].

use std::ffi::OsStr;
use std::sync::{Arc, OnceLock};

/// The agent type, named here and nowhere else, so that `ureq::Agent` written outside this file
/// is the one thing the check over the crate's source has to look for.
pub(crate) use ureq::Agent;
use ureq::rustls::{self, ClientConfig, RootCertStore};

/// An agent builder that trusts the system's roots, for a caller to add its own headers and
/// timeouts to.
pub(crate) fn agent_builder() -> ureq::AgentBuilder {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    ureq::AgentBuilder::new().tls_config(CONFIG.get_or_init(client_config).clone())
}

/// The provider and protocol versions are the ones `ureq` picks for itself, named here because
/// a config built outside it does not inherit them.
fn client_config() -> Arc<ClientConfig> {
    let config =
        ClientConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
            .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
            .expect("the ring provider offers TLS 1.2 and 1.3")
            .with_root_certificates(roots())
            .with_no_client_auth();
    Arc::new(config)
}

/// Whatever the system holds that parses. A root that does not is left out. A store that ends up
/// empty makes every request fail as an invalid peer certificate, which says nothing of the file
/// that was missing, so the cause is written once to standard error then.
fn roots() -> RootCertStore {
    let loaded = rustls_native_certs::load_native_certs();
    let mut store = RootCertStore::empty();
    let (added, _) = store.add_parsable_certificates(loaded.certs);
    if added == 0 {
        let errors: Vec<String> = loaded.errors.iter().map(ToString::to_string).collect();
        eprintln!(
            "{}",
            no_roots(
                &errors,
                std::env::var_os("SSL_CERT_FILE").as_deref(),
                std::env::var_os("SSL_CERT_DIR").as_deref(),
            )
        );
    }
    store
}

/// The text for a store with no root in it: what the loader reported, and the two variables that
/// steer it where they are set, since a path that is not there is the usual cause and the loader's
/// own message does not always carry it.
fn no_roots(errors: &[String], file: Option<&OsStr>, dir: Option<&OsStr>) -> String {
    let mut text = String::from(
        "homma: no trusted root certificate could be read, so every HTTPS request will be refused",
    );
    for (name, value) in [("SSL_CERT_FILE", file), ("SSL_CERT_DIR", dir)] {
        if let Some(value) = value {
            text.push_str(&format!("\n  {name} is {}", value.to_string_lossy()));
        }
    }
    for error in errors {
        text.push_str(&format!("\n  {error}"));
    }
    text
}

#[cfg(test)]
#[path = "tls_tests.rs"]
mod tests;
