use std::sync::Arc;

use opentelemetry::{
    metrics::{Counter, Histogram, Meter, UpDownCounter},
    KeyValue,
};

use crate::{
    gql_response_status::{GraphqlResponseStatus, SubgraphResponseStatus},
    grafbase_client::Client,
};

pub struct EngineMetrics {
    graph_version: Option<String>,
    operation_latency: Histogram<u64>,
    subgraph_latency: Histogram<u64>,
    subgraph_retries: Counter<u64>,
    subgraph_request_body_size: Histogram<u64>,
    subgraph_response_body_size: Histogram<u64>,
    subgraph_requests_inflight: UpDownCounter<i64>,
    subgraph_cache_hits: Counter<u64>,
    subgraph_cache_misses: Counter<u64>,
    operation_cache_hits: Counter<u64>,
    operation_cache_misses: Counter<u64>,
    query_preparation_latency: Histogram<u64>,
    batch_sizes: Histogram<u64>,
    request_body_sizes: Histogram<u64>,
    graphql_errors: Counter<u64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum OperationType {
    Query,
    Mutation,
    Subscription,
}

impl OperationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OperationMetricsAttributes {
    pub ty: OperationType,
    pub name: Option<String>,
    pub sanitized_query: Arc<str>,
}

#[derive(Debug)]
pub struct GraphqlRequestMetricsAttributes {
    pub operation: OperationMetricsAttributes,
    pub status: GraphqlResponseStatus,
    pub cache_status: Option<String>,
    pub client: Option<Client>,
}

#[derive(Debug)]
pub struct SubgraphRequestDurationAttributes {
    pub name: String,
    pub subgraph_status: SubgraphResponseStatus,
    pub status_code: Option<http::StatusCode>,
}

#[derive(Debug)]
pub struct SubgraphRequestRetryAttributes {
    pub name: String,
    pub aborted: bool,
}

#[derive(Debug)]
pub struct SubgraphRequestBodySizeAttributes {
    pub name: String,
}

#[derive(Debug)]
pub struct SubgraphResponseBodySizeAttributes {
    pub name: String,
}

#[derive(Debug)]
pub struct SubgraphInFlightRequestAttributes {
    pub name: String,
}

#[derive(Debug)]
pub struct SubgraphCacheHitAttributes {
    pub name: String,
}

#[derive(Debug)]
pub struct SubgraphCacheMissAttributes {
    pub name: String,
}

#[derive(Debug)]
pub struct QueryPreparationAttributes {
    pub operation: OperationMetricsAttributes,
    pub success: bool,
}

#[derive(Debug)]
pub struct GraphqlErrorAttributes {
    pub code: &'static str,
    pub operation_name: Option<String>,
    pub client: Option<Client>,
}

impl EngineMetrics {
    pub fn build(meter: &Meter, graph_version: Option<String>) -> Self {
        Self {
            graph_version,
            operation_latency: meter.u64_histogram("graphql.operation.duration").init(),
            subgraph_latency: meter.u64_histogram("graphql.subgraph.request.duration").init(),
            subgraph_retries: meter.u64_counter("graphql.subgraph.request.retries").init(),
            subgraph_request_body_size: meter.u64_histogram("graphql.subgraph.request.body.size").init(),
            subgraph_response_body_size: meter.u64_histogram("graphql.subgraph.response.body.size").init(),
            subgraph_requests_inflight: meter.i64_up_down_counter("graphql.subgraph.request.inflight").init(),
            subgraph_cache_hits: meter.u64_counter("graphql.subgraph.request.cache.hit").init(),
            subgraph_cache_misses: meter.u64_counter("graphql.subgraph.request.cache.miss").init(),
            operation_cache_hits: meter.u64_counter("graphql.operation.cache.hit").init(),
            operation_cache_misses: meter.u64_counter("graphql.operation.cache.miss").init(),
            query_preparation_latency: meter.u64_histogram("graphql.operation.prepare.duration").init(),
            batch_sizes: meter.u64_histogram("graphql.operation.batch.size").init(),
            request_body_sizes: meter.u64_histogram("http.server.request.body.size").init(),
            graphql_errors: meter.u64_counter("graphql.operation.errors").init(),
        }
    }

    fn create_operation_key_values(&self, operation: OperationMetricsAttributes) -> Vec<KeyValue> {
        let mut attributes = vec![
            KeyValue::new("graphql.document", operation.sanitized_query.clone()),
            KeyValue::new("graphql.operation.type", operation.ty.as_str()),
        ];

        if let Some(name) = operation.name {
            attributes.push(KeyValue::new("graphql.operation.name", name));
        }

        attributes
    }

    pub fn record_operation_duration(
        &self,
        GraphqlRequestMetricsAttributes {
            operation,
            status,
            cache_status,
            client,
        }: GraphqlRequestMetricsAttributes,
        latency: std::time::Duration,
    ) {
        let mut attributes = self.create_operation_key_values(operation);

        if let Some(version) = self.graph_version.clone() {
            attributes.push(KeyValue::new("grafbase.graph.version", version))
        }

        // Used for v1
        if let Some(cache_status) = cache_status {
            attributes.push(KeyValue::new("graphql.response.cache.status", cache_status));
        }

        attributes.push(KeyValue::new("graphql.response.status", status.as_str()));

        if let Some(client) = client {
            attributes.push(KeyValue::new("http.headers.x-grafbase-client-name", client.name));

            if let Some(version) = client.version {
                attributes.push(KeyValue::new("http.headers.x-grafbase-client-version", version));
            }
        }

        self.operation_latency.record(latency.as_millis() as u64, &attributes);
    }

    pub fn record_subgraph_duration(
        &self,
        SubgraphRequestDurationAttributes {
            name,
            subgraph_status: status,
            status_code,
        }: SubgraphRequestDurationAttributes,
        latency: std::time::Duration,
    ) {
        let mut attributes = vec![
            KeyValue::new("graphql.subgraph.name", name),
            KeyValue::new("graphql.subgraph.response.status", status.as_str()),
        ];

        if let Some(status_code) = status_code {
            attributes.push(KeyValue::new("http.response.status_code", status_code.as_u16() as i64));
        }

        self.subgraph_latency.record(latency.as_millis() as u64, &attributes);
    }

    pub fn record_subgraph_retry(
        &self,
        SubgraphRequestRetryAttributes { name, aborted }: SubgraphRequestRetryAttributes,
    ) {
        let attributes = [
            KeyValue::new("graphql.subgraph.name", name),
            KeyValue::new("graphql.subgraph.aborted", aborted),
        ];

        self.subgraph_retries.add(1, &attributes);
    }

    pub fn record_subgraph_request_size(
        &self,
        SubgraphRequestBodySizeAttributes { name }: SubgraphRequestBodySizeAttributes,
        size: usize,
    ) {
        let attributes = [KeyValue::new("graphql.subgraph.name", name)];
        self.subgraph_request_body_size.record(size as u64, &attributes);
    }

    pub fn record_subgraph_response_size(
        &self,
        SubgraphResponseBodySizeAttributes { name }: SubgraphResponseBodySizeAttributes,
        size: usize,
    ) {
        let attributes = [KeyValue::new("graphql.subgraph.name", name)];
        self.subgraph_response_body_size.record(size as u64, &attributes);
    }

    pub fn increment_subgraph_inflight_requests(
        &self,
        SubgraphInFlightRequestAttributes { name }: SubgraphInFlightRequestAttributes,
    ) {
        let attributes = [KeyValue::new("graphql.subgraph.name", name)];
        self.subgraph_requests_inflight.add(1, &attributes);
    }

    pub fn decrement_subgraph_inflight_requests(
        &self,
        SubgraphInFlightRequestAttributes { name }: SubgraphInFlightRequestAttributes,
    ) {
        let attributes = [KeyValue::new("graphql.subgraph.name", name)];
        self.subgraph_requests_inflight.add(-1, &attributes);
    }

    pub fn record_subgraph_cache_hit(&self, SubgraphCacheHitAttributes { name }: SubgraphCacheHitAttributes) {
        let attributes = [KeyValue::new("graphql.subgraph.name", name)];
        self.subgraph_cache_hits.add(1, &attributes);
    }

    pub fn record_subgraph_cache_miss(&self, SubgraphCacheMissAttributes { name }: SubgraphCacheMissAttributes) {
        let attributes = [KeyValue::new("graphql.subgraph.name", name)];
        self.subgraph_cache_misses.add(1, &attributes);
    }

    pub fn record_operation_cache_hit(&self) {
        self.operation_cache_hits.add(1, &[]);
    }

    pub fn record_operation_cache_miss(&self) {
        self.operation_cache_misses.add(1, &[]);
    }

    pub fn record_preparation_latency(
        &self,
        QueryPreparationAttributes { operation, success }: QueryPreparationAttributes,
        latency: std::time::Duration,
    ) {
        let mut attributes = self.create_operation_key_values(operation);

        attributes.push(KeyValue::new("graphql.operation.success", success));

        self.query_preparation_latency
            .record(latency.as_millis() as u64, &attributes);
    }

    pub fn record_batch_size(&self, size: usize) {
        self.batch_sizes.record(size as u64, &[]);
    }

    pub fn record_request_body_size(&self, size: usize) {
        self.request_body_sizes.record(size as u64, &[]);
    }

    pub fn increment_graphql_errors(
        &self,
        GraphqlErrorAttributes {
            code,
            operation_name,
            client,
        }: GraphqlErrorAttributes,
    ) {
        let mut attributes = vec![KeyValue::new("graphql.response.error.code", code)];

        if let Some(name) = operation_name {
            attributes.push(KeyValue::new("graphql.operation.name", name));
        }

        if let Some(client) = client {
            attributes.push(KeyValue::new("http.headers.x-grafbase-client-name", client.name));

            if let Some(version) = client.version {
                attributes.push(KeyValue::new("http.headers.x-grafbase-client-version", version));
            }
        }

        self.graphql_errors.add(1, &attributes);
    }
}
