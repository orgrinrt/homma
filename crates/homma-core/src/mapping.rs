//! Bridge from homma's workspace-level config onto mockspace's canonical Config.
//!
//! Homma operates at the workspace level (many repos, one config). Mockspace
//! operates at the per-repo level (one repo's templates and lints). The bridge
//! maps the subset of homma's `[workspace]` that mockspace's template context
//! cares about (`project_name`, `repo_root`). Everything else defaults.
//!
//! When homma needs to invoke mockspace template rendering, it converts via
//! [`IntoMockspaceConfig`] and hands the result to mockspace-template.

use mockspace_config::{Config as MockspaceConfig, IntoMockspaceConfig, MappingError};

use crate::config::Config;

impl IntoMockspaceConfig for Config {
    fn into_mockspace_config(self) -> Result<MockspaceConfig, MappingError> {
        Ok(MockspaceConfig {
            project_name: self.workspace.name,
            repo_root: self.workspace.path,
            ..MockspaceConfig::default()
        })
    }
}
