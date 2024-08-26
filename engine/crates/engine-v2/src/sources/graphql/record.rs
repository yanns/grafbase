use grafbase_telemetry::{
    gql_response_status::SubgraphResponseStatus,
    metrics::{
        SubgraphCacheHitAttributes, SubgraphCacheMissAttributes, SubgraphInFlightRequestAttributes,
        SubgraphRequestBodySizeAttributes, SubgraphRequestDurationAttributes, SubgraphRequestRetryAttributes,
        SubgraphResponseBodySizeAttributes,
    },
};
use schema::sources::graphql::GraphqlEndpointWalker;
use web_time::Duration;

use crate::{execution::ExecutionContext, Runtime};

pub(super) fn subgraph_retry<R: Runtime>(
    ctx: ExecutionContext<'_, R>,
    endpoint: GraphqlEndpointWalker<'_>,
    aborted: bool,
) {
    ctx.engine
        .metrics
        .record_subgraph_retry(SubgraphRequestRetryAttributes {
            name: endpoint.subgraph_name().to_string(),
            aborted,
        });
}

pub(super) fn subgraph_duration<R: Runtime>(
    ctx: ExecutionContext<'_, R>,
    endpoint: GraphqlEndpointWalker<'_>,
    subgraph_status: SubgraphResponseStatus,
    status_code: Option<http::StatusCode>,
    duration: Duration,
) {
    ctx.engine.metrics.record_subgraph_duration(
        SubgraphRequestDurationAttributes {
            name: endpoint.subgraph_name().to_string(),
            subgraph_status,
            status_code,
        },
        duration,
    );
}

pub(super) fn subgraph_request_size<R: Runtime>(
    ctx: ExecutionContext<'_, R>,
    endpoint: GraphqlEndpointWalker<'_>,
    size: usize,
) {
    ctx.engine.metrics.record_subgraph_request_size(
        SubgraphRequestBodySizeAttributes {
            name: endpoint.subgraph_name().to_string(),
        },
        size,
    );
}

pub(super) fn subgraph_response_size<R: Runtime>(
    ctx: ExecutionContext<'_, R>,
    endpoint: GraphqlEndpointWalker<'_>,
    size: usize,
) {
    ctx.engine.metrics.record_subgraph_response_size(
        SubgraphResponseBodySizeAttributes {
            name: endpoint.subgraph_name().to_string(),
        },
        size,
    );
}

pub(super) fn increment_inflight_requests<R: Runtime>(
    ctx: ExecutionContext<'_, R>,
    endpoint: GraphqlEndpointWalker<'_>,
) {
    ctx.engine
        .metrics
        .increment_subgraph_inflight_requests(SubgraphInFlightRequestAttributes {
            name: endpoint.subgraph_name().to_string(),
        });
}

pub(super) fn decrement_inflight_requests<R: Runtime>(
    ctx: ExecutionContext<'_, R>,
    endpoint: GraphqlEndpointWalker<'_>,
) {
    ctx.engine
        .metrics
        .decrement_subgraph_inflight_requests(SubgraphInFlightRequestAttributes {
            name: endpoint.subgraph_name().to_string(),
        });
}

pub(super) fn record_subgraph_cache_hit<R: Runtime>(ctx: ExecutionContext<'_, R>, endpoint: GraphqlEndpointWalker<'_>) {
    ctx.engine
        .metrics
        .record_subgraph_cache_hit(SubgraphCacheHitAttributes {
            name: endpoint.subgraph_name().to_string(),
        });
}

pub(super) fn record_subgraph_cache_miss<R: Runtime>(
    ctx: ExecutionContext<'_, R>,
    endpoint: GraphqlEndpointWalker<'_>,
) {
    ctx.engine
        .metrics
        .record_subgraph_cache_miss(SubgraphCacheMissAttributes {
            name: endpoint.subgraph_name().to_string(),
        });
}
