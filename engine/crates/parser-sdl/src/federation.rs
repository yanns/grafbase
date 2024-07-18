pub mod header;

use std::collections::BTreeMap;
use std::time::Duration;

use crate::{rules::auth_directive::v2::AuthV2Directive, GlobalCacheRules};
use registry_v2::{ConnectorHeaderValue, OperationLimits};

use self::header::{NameOrPattern, SubgraphHeaderForward, SubgraphHeaderInsert, SubgraphHeaderRule};

/// Configuration for a federated graph
#[derive(Clone, Debug, Default)]
pub struct FederatedGraphConfig {
    pub subgraphs: BTreeMap<String, SubgraphConfig>,
    pub header_rules: Vec<SubgraphHeaderRule>,
    pub operation_limits: OperationLimits,
    pub global_cache_rules: GlobalCacheRules<'static>,
    pub auth: Option<AuthV2Directive>,
    pub disable_introspection: bool,
    pub rate_limit: Option<RateLimitConfig>,
}

/// Configuration for a subgraph of the current federated graph
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct SubgraphConfig {
    /// The name of the subgrah
    pub name: String,

    /// The URL to use in development
    ///
    /// This is only used in development and should be ignored in deployed
    /// environments
    pub development_url: Option<String>,

    /// The URL to use for GraphQL-WS calls.
    ///
    /// This will default to the normal URL if not present.
    pub websocket_url: Option<String>,

    /// Rules for passing headers forward to the subgraph
    pub header_rules: Vec<SubgraphHeaderRule>,

    /// Configuration to enforce rate limiting on subgraph requests
    pub rate_limit: Option<RateLimitConfig>,
}

impl From<(String, ConnectorHeaderValue)> for SubgraphHeaderRule {
    fn from((name, value): (String, ConnectorHeaderValue)) -> Self {
        match value {
            ConnectorHeaderValue::Static(value) => SubgraphHeaderRule::Insert(SubgraphHeaderInsert { name, value }),
            ConnectorHeaderValue::Forward(value) => SubgraphHeaderRule::Forward(SubgraphHeaderForward {
                name: NameOrPattern::Name(value),
                default: None,
                rename: Some(name),
            }),
        }
    }
}

// we're simplifying federated rate limiting atm, taking the same config (registry_v2::rate_limiting::RateLimitConfig)
// for standalone v1 and local wouldn't work as its quite different
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RateLimitConfig {
    pub limit: usize,
    pub duration: Duration,
}
