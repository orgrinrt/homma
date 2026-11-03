//--------------------------------------------------------------------------------------------------
// Copyright (c) 2026                   orgrinrt                 ort@hiisi.digital
// SPDX-License-Identifier: MPL-2.0     https://mozilla.org/MPL/2.0        contact@hiisi.digital
//--------------------------------------------------------------------------------------------------
//! The one place an HTTP agent here gets the roots it trusts.
//!
//! The roots are the system's, and the ones `SSL_CERT_FILE` or `SSL_CERT_DIR` name where set,
//! which is how a cloud container's proxy authority reaches the registry indexes and the forges.
//! They are read once for the process, and every agent starts from [`agent_builder`].

use std::sync::{Arc, OnceLock};

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

/// Whatever the system holds that parses. A root that does not is left out, and a store that
/// ends up empty makes every request fail as an unknown issuer, which names the cause.
fn roots() -> RootCertStore {
    let mut store = RootCertStore::empty();
    store.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    store
}

#[cfg(test)]
#[path = "tls_tests.rs"]
mod tests;
