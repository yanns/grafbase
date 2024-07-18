use grafbase_telemetry::span::subgraph::SubgraphRequestSpan;
use runtime::fetch::FetchRequest;
use schema::sources::graphql::{FederationEntityResolverWalker, GraphqlEndpointId, GraphqlEndpointWalker};
use serde::de::DeserializeSeed;
use tracing::Instrument;

use crate::{
    execution::{ExecutionContext, PlanWalker, PlanningResult},
    operation::OperationType,
    response::ResponsePart,
    sources::{
        graphql::deserialize::{EntitiesErrorsSeed, GraphqlResponseSeed},
        ExecutionResult, Executor, ExecutorInput, PreparedExecutor,
    },
    Runtime,
};

use super::{
    deserialize::EntitiesDataSeed, query::PreparedFederationEntityOperation, request::execute_subgraph_request,
    variables::SubgraphVariables,
};

pub(crate) struct FederationEntityPreparedExecutor {
    subgraph_id: GraphqlEndpointId,
    operation: PreparedFederationEntityOperation,
}

impl FederationEntityPreparedExecutor {
    pub fn prepare(
        resolver: FederationEntityResolverWalker<'_>,
        plan: PlanWalker<'_>,
    ) -> PlanningResult<PreparedExecutor> {
        let subgraph = resolver.endpoint();
        let operation =
            PreparedFederationEntityOperation::build(plan).map_err(|err| format!("Failed to build query: {err}"))?;
        Ok(PreparedExecutor::FederationEntity(Self {
            subgraph_id: subgraph.id(),
            operation,
        }))
    }

    pub fn new_executor<'ctx, R: Runtime>(
        &'ctx self,
        input: ExecutorInput<'ctx, '_, R>,
    ) -> ExecutionResult<Executor<'ctx, R>> {
        let ExecutorInput {
            ctx,
            plan,
            root_response_objects,
        } = input;

        let root_response_objects = root_response_objects.with_extra_constant_fields(vec![(
            "__typename".to_string(),
            serde_json::Value::String(
                ctx.engine
                    .schema
                    .walker()
                    .walk(schema::Definition::from(plan.logical_plan().as_ref().entity_id))
                    .name()
                    .to_string(),
            ),
        )]);
        let variables = SubgraphVariables {
            plan,
            variables: &self.operation.variables,
            inputs: vec![(&self.operation.entities_variable_name, root_response_objects)],
        };

        let subgraph = ctx.engine.schema.walk(self.subgraph_id);
        tracing::debug!(
            "Query {}\n{}\n{}",
            subgraph.name(),
            self.operation.query,
            serde_json::to_string_pretty(&variables).unwrap_or_default()
        );
        let json_body = serde_json::to_string(&serde_json::json!({
            "query": self.operation.query,
            "variables": variables
        }))
        .map_err(|err| format!("Failed to serialize query: {err}"))?;

        Ok(Executor::FederationEntity(FederationEntityExecutor {
            ctx,
            subgraph,
            operation: &self.operation,
            json_body,
            plan,
        }))
    }
}

pub(crate) struct FederationEntityExecutor<'ctx, R: Runtime> {
    ctx: ExecutionContext<'ctx, R>,
    subgraph: GraphqlEndpointWalker<'ctx>,
    operation: &'ctx PreparedFederationEntityOperation,
    json_body: String,
    plan: PlanWalker<'ctx>,
}

impl<'ctx, R: Runtime> FederationEntityExecutor<'ctx, R> {
    #[tracing::instrument(skip_all)]
    pub async fn execute(self, mut response_part: ResponsePart) -> ExecutionResult<ResponsePart> {
        let span = SubgraphRequestSpan {
            name: self.subgraph.name(),
            operation_type: OperationType::Query.as_str(),
            // The generated query does not contain any data, everything are in the variables, so
            // it's safe to use.
            sanitized_query: &self.operation.query,
            url: self.subgraph.url(),
        }
        .into_span();

        execute_subgraph_request(
            self.ctx,
            span.clone(),
            self.subgraph.name(),
            || FetchRequest {
                url: self.subgraph.url(),
                headers: self.ctx.headers_with_rules(self.subgraph.header_rules()),
                json_body: self.json_body,
                subgraph_name: self.subgraph.name(),
                timeout: self.subgraph.timeout(),
            },
            move |bytes| {
                let part = response_part.as_mut();
                let status = GraphqlResponseSeed::new(
                    EntitiesDataSeed {
                        response_part: &part,
                        plan: self.plan,
                    },
                    EntitiesErrorsSeed {
                        response_part: &part,
                        response_keys: self.plan.response_keys(),
                    },
                )
                .deserialize(&mut serde_json::Deserializer::from_slice(&bytes))?;
                Ok((status, response_part))
            },
        )
        .instrument(span)
        .await
    }
}
