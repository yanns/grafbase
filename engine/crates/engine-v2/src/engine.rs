use ::runtime::{
    auth::AccessToken,
    hooks::Hooks,
    hot_cache::{CachedDataKind, HotCache, HotCacheFactory},
};
use async_runtime::stream::StreamExt as _;
use engine::{BatchRequest, Request};
use engine_parser::types::OperationType;
use futures::{channel::mpsc, FutureExt, StreamExt};
use futures_util::{SinkExt, Stream};
use gateway_core::StreamingFormat;
use gateway_v2_auth::AuthService;
use grafbase_tracing::{
    gql_response_status::GraphqlResponseStatus,
    grafbase_client::Client,
    metrics::{GraphqlOperationMetrics, GraphqlOperationMetricsAttributes},
    span::{gql::GqlRequestSpan, GqlRecorderSpanExt, GqlRequestAttributes},
};
use headers::HeaderMapExt;
use schema::Schema;
use std::{borrow::Cow, sync::Arc};
use tracing::Instrument;
use trusted_documents::PreparedOperationDocument;
use web_time::Instant;

use crate::{
    execution::PreExecutionContext,
    http_response::{HttpGraphqlResponse, HttpGraphqlResponseExtraMetadata},
    operation::{Operation, OperationMetadata, Variables},
    plan::OperationPlan,
    response::{ErrorCode, GraphqlError, Response},
    websocket,
};

mod cache;
mod runtime;
mod trusted_documents;

pub use runtime::Runtime;

pub(crate) struct SchemaVersion(Vec<u8>);

impl std::ops::Deref for SchemaVersion {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

pub struct Engine<R: Runtime> {
    // We use an Arc for the schema to have a self-contained response which may still
    // needs access to the schema strings
    pub(crate) schema: Arc<Schema>,
    pub(crate) schema_version: SchemaVersion,
    pub(crate) runtime: R,
    operation_metrics: GraphqlOperationMetrics,
    auth: AuthService,
    trusted_documents_cache: <R::CacheFactory as HotCacheFactory>::Cache<String>,
    operation_cache: <R::CacheFactory as HotCacheFactory>::Cache<Arc<Operation>>,
}

impl<R: Runtime> Engine<R> {
    /// schema_version is used in operation cache key which ensures we only retrieve cached
    /// operation for the same schema version. If none is provided, a random one is generated.
    pub async fn new(schema: Arc<Schema>, schema_version: Option<&[u8]>, runtime: R) -> Self {
        let auth = gateway_v2_auth::AuthService::new_v2(
            schema.settings.auth_config.clone().unwrap_or_default(),
            runtime.kv().clone(),
        );

        Self {
            schema,
            schema_version: SchemaVersion({
                let mut out = Vec::new();
                match schema_version {
                    Some(version) => {
                        out.push(0x00);
                        out.extend_from_slice(version);
                    }
                    None => {
                        out.push(0x01);
                        out.extend_from_slice(&ulid::Ulid::new().to_bytes());
                    }
                }
                out
            }),
            auth,
            operation_metrics: GraphqlOperationMetrics::build(runtime.meter()),
            trusted_documents_cache: runtime.cache_factory().create(CachedDataKind::PersistedQuery).await,
            operation_cache: runtime.cache_factory().create(CachedDataKind::Operation).await,
            runtime,
        }
    }

    pub async fn execute(
        self: &Arc<Self>,
        headers: http::HeaderMap,
        batch_request: BatchRequest,
    ) -> HttpGraphqlResponse {
        let (hooks_context, headers) = match self.runtime.hooks().on_gateway_request(headers).await {
            Ok(result) => result,
            Err(error) => return Response::pre_execution_error(error).into(),
        };

        if let Some(access_token) = self.auth.authorize(&headers).await {
            let context = RequestContext::new(headers, access_token, hooks_context);
            self.execute_with_access_token(context, batch_request).await
        } else if let Some(streaming_format) = headers.typed_get::<StreamingFormat>() {
            HttpGraphqlResponse::stream_request_error(streaming_format, "Unauthorized")
        } else {
            HttpGraphqlResponse::request_error("Unauthorized")
        }
    }

    pub async fn create_session(self: &Arc<Self>, headers: http::HeaderMap) -> Result<Session<R>, Cow<'static, str>> {
        let (hooks_context, headers) = match self.runtime.hooks().on_gateway_request(headers).await {
            Ok(result) => result,
            Err(error) => return Err(Cow::from(error.to_string())),
        };

        match self.auth.authorize(&headers).await {
            Some(access_token) => Ok(Session {
                engine: Arc::clone(self),
                request_context: Arc::new(RequestContext::new(headers, access_token, hooks_context)),
            }),
            None => Err(Cow::from("Forbidden")),
        }
    }

    async fn execute_with_access_token(
        self: &Arc<Self>,
        request_context: RequestContext<<R::Hooks as Hooks>::Context>,
        batch_request: BatchRequest,
    ) -> HttpGraphqlResponse {
        let streaming_format = request_context.headers.typed_get::<StreamingFormat>();
        match batch_request {
            BatchRequest::Single(request) => {
                if let Some(streaming_format) = streaming_format {
                    convert_stream_to_http_response(
                        streaming_format,
                        self.execute_stream(Arc::new(request_context), request),
                    )
                    .await
                } else {
                    self.execute_single(&request_context, request).await
                }
            }
            BatchRequest::Batch(requests) => {
                if streaming_format.is_some() {
                    return HttpGraphqlResponse::request_error(
                        "batch requests can't use multipart or event-stream responses",
                    );
                }
                HttpGraphqlResponse::batch_response(
                    futures_util::stream::iter(requests.into_iter())
                        .then(|request| self.execute_single(&request_context, request))
                        .collect::<Vec<_>>()
                        .await,
                )
            }
        }
    }

    async fn execute_single(
        &self,
        request_context: &RequestContext<<R::Hooks as Hooks>::Context>,
        request: Request,
    ) -> HttpGraphqlResponse {
        let start = Instant::now();
        let span = GqlRequestSpan::new().into_span();
        async {
            let ctx = PreExecutionContext::new(self, request_context);
            let (operation_metadata, response) = ctx.execute_single(request).await;
            let status = response.status();
            let mut response_metadata = HttpGraphqlResponseExtraMetadata {
                operation_name: None,
                operation_type: None,
                has_errors: !status.is_success(),
            };
            if let Some(OperationMetadata {
                ty,
                name,
                normalized_query,
                normalized_query_hash,
            }) = operation_metadata
            {
                span.record_gql_request(GqlRequestAttributes {
                    operation_type: ty.as_str(),
                    operation_name: name.clone(),
                });
                response_metadata.operation_name.clone_from(&name);
                response_metadata.operation_type = Some(ty.as_str());
                self.operation_metrics.record(
                    GraphqlOperationMetricsAttributes {
                        ty: ty.as_str(),
                        name,
                        normalized_query,
                        normalized_query_hash,
                        status,
                        cache_status: None,
                        client: request_context.client.clone(),
                    },
                    start.elapsed(),
                );
            }
            span.record_gql_status(status);
            HttpGraphqlResponse::from(response).with_metadata(response_metadata)
        }
        .instrument(span.clone())
        .await
    }

    fn execute_stream(
        self: &Arc<Self>,
        request_context: Arc<RequestContext<<R::Hooks as Hooks>::Context>>,
        request: Request,
    ) -> impl Stream<Item = Response> + Send + 'static {
        let start = Instant::now();
        let engine = Arc::clone(self);
        let (sender, receiver) = mpsc::channel(2);

        let span = GqlRequestSpan::new().into_span();
        let span_clone = span.clone();
        receiver.join(
            async move {
                let ctx = PreExecutionContext::new(&engine, &request_context);
                let (metadata, status) = ctx.execute_stream(request, sender).await;
                if let Some(OperationMetadata {
                    ty,
                    name,
                    normalized_query,
                    normalized_query_hash,
                }) = metadata
                {
                    span.record_gql_request(GqlRequestAttributes {
                        operation_type: ty.as_str(),
                        operation_name: name.clone(),
                    });
                    engine.operation_metrics.record(
                        GraphqlOperationMetricsAttributes {
                            ty: ty.as_str(),
                            name,
                            normalized_query,
                            normalized_query_hash,
                            status,
                            cache_status: None,
                            client: request_context.client.clone(),
                        },
                        start.elapsed(),
                    );
                }

                span.record_gql_status(status);
            }
            .instrument(span_clone),
        )
    }
}

async fn convert_stream_to_http_response(
    streaming_format: StreamingFormat,
    stream: impl Stream<Item = Response> + Send + 'static,
) -> HttpGraphqlResponse {
    let mut stream = Box::pin(stream);
    let Some(first_response) = stream.next().await else {
        return HttpGraphqlResponse::request_error("Empty stream");
    };
    HttpGraphqlResponse::from_stream(
        streaming_format,
        // Not perfect for the errors count, but good enough to detect a request error
        first_response.status(),
        futures_util::stream::iter(std::iter::once(first_response)).chain(stream),
    )
}

impl<'ctx, R: Runtime> PreExecutionContext<'ctx, R> {
    async fn execute_single(mut self, mut request: Request) -> (Option<OperationMetadata>, Response) {
        let operation = match self.prepare_operation(&mut request).await {
            Ok(operation) => operation,
            Err((metadata, response)) => return (metadata, response),
        };

        let metadata = Some(operation.metadata.clone());
        let response = if matches!(operation.ty(), OperationType::Subscription) {
            Response::pre_execution_error(GraphqlError::new(
                "Subscriptions are only suported on streaming transports. Try making a request with SSE or WebSockets",
                ErrorCode::BadRequest,
            ))
        } else {
            match self.finalize_operation(operation, request.variables).await {
                Ok(operation_plan) => self.execute_query_or_mutation(operation_plan).await,
                Err(errors) => Response::pre_execution_errors(errors),
            }
        };

        (metadata, response)
    }

    async fn execute_stream(
        mut self,
        mut request: Request,
        mut sender: mpsc::Sender<Response>,
    ) -> (Option<OperationMetadata>, GraphqlResponseStatus) {
        let operation = match self.prepare_operation(&mut request).await {
            Ok(operation) => operation,
            Err((metadata, response)) => {
                let status = response.status();
                sender.send(response).await.ok();
                return (metadata, status);
            }
        };
        let operation_type = operation.ty();
        let metadata = Some(operation.metadata.clone());

        let operation_plan = match self.finalize_operation(operation, request.variables).await {
            Ok(operation_plan) => operation_plan,
            Err(errors) => {
                let response = Response::pre_execution_errors(errors);
                let status = response.status();
                sender.send(response).await.ok();
                return (metadata, status);
            }
        };

        if matches!(operation_type, OperationType::Query | OperationType::Mutation) {
            let response = self.execute_query_or_mutation(operation_plan).await;
            let status = response.status();
            sender.send(response).await.ok();
            return (metadata, status);
        }

        let mut status: GraphqlResponseStatus = GraphqlResponseStatus::Success;
        struct Sender<'a> {
            sender: mpsc::Sender<Response>,
            status: &'a mut GraphqlResponseStatus,
        }

        impl crate::execution::ResponseSender for Sender<'_> {
            type Error = mpsc::SendError;
            async fn send(&mut self, response: Response) -> Result<(), Self::Error> {
                *self.status = self.status.union(response.status());
                self.sender.send(response).await
            }
        }

        self.execute_subscription(
            operation_plan,
            Sender {
                sender,
                status: &mut status,
            },
        )
        .await;
        (metadata, status)
    }

    async fn prepare_operation(
        &mut self,
        request: &mut Request,
    ) -> Result<Arc<Operation>, (Option<OperationMetadata>, Response)> {
        let (cache_key, query) = {
            let PreparedOperationDocument {
                cache_key,
                document_fut,
            } = match self.prepare_operation_document(request) {
                Ok(pq) => pq,
                Err(err) => return Err((None, Response::pre_execution_error(err))),
            };
            if let Some(operation) = self.operation_cache.get(&cache_key).await {
                return Ok(operation);
            }
            if let Some(persisted_query) = document_fut {
                match persisted_query.await {
                    Ok(query) => (cache_key, Some(query)),
                    Err(err) => return Err((None, Response::pre_execution_error(err))),
                }
            } else {
                (cache_key, None)
            }
        };
        if let Some(query) = query {
            request.query = query
        }

        let operation = Operation::build(&self.schema, request)
            .map(Arc::new)
            .map_err(|mut err| (err.take_operation_metadata(), Response::pre_execution_error(err)))?;

        self.push_background_future(self.engine.operation_cache.insert(cache_key, operation.clone()).boxed());

        Ok(operation)
    }

    async fn finalize_operation(
        &self,
        operation: Arc<Operation>,
        variables: engine::Variables,
    ) -> Result<OperationPlan, Vec<GraphqlError>> {
        let variables = Variables::build(self.schema.as_ref(), &operation, variables)
            .map_err(|errors| errors.into_iter().map(Into::into).collect::<Vec<_>>())?;

        OperationPlan::build(self, operation, variables)
            .await
            .map_err(|err| vec![err.into()])
    }
}

pub struct Session<R: Runtime> {
    engine: Arc<Engine<R>>,
    request_context: Arc<RequestContext<<R::Hooks as Hooks>::Context>>,
}

impl<R: Runtime> Clone for Session<R> {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
            request_context: Arc::clone(&self.request_context),
        }
    }
}

pub(crate) struct RequestContext<C> {
    pub headers: http::HeaderMap,
    pub client: Option<Client>,
    pub access_token: AccessToken,
    pub hooks_context: C,
}

impl<C> RequestContext<C> {
    fn new(headers: http::HeaderMap, access_token: AccessToken, hooks_context: C) -> Self {
        let client = Client::extract_from(&headers);

        Self {
            headers,
            client,
            access_token,
            hooks_context,
        }
    }
}

impl<R: Runtime> Session<R> {
    pub fn execute_websocket(&self, id: String, request: Request) -> impl Stream<Item = websocket::Message> {
        self.engine
            .execute_stream(self.request_context.clone(), request)
            .map(move |response| match response {
                Response::PreExecutionError(_) => websocket::Message::Error {
                    id: id.clone(),
                    payload: websocket::Payload(response),
                },
                response => websocket::Message::Next {
                    id: id.clone(),
                    payload: websocket::Payload(response),
                },
            })
    }
}
