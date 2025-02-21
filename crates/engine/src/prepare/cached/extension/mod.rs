mod query;
mod response;
mod template;

use operation::Variables;
pub(crate) use query::*;
pub(crate) use response::*;
use schema::{ExtensionDirective, InjectionStage, Schema};

use crate::response::ResponseObjectsView;

use super::PartitionFieldArguments;

#[derive(Clone, Copy)]
pub(crate) struct ArgumentsContext<'a> {
    schema: &'a Schema,
    field_arguments: PartitionFieldArguments<'a>,
    variables: &'a Variables,
}

pub(crate) fn create_extension_directive_arguments_view<'ctx>(
    schema: &'ctx Schema,
    directive: ExtensionDirective<'ctx>,
    field_arguments: PartitionFieldArguments<'ctx>,
    variables: &'ctx Variables,
) -> ExtensionDirectiveArgumentsQueryView<'ctx> {
    let ctx = ArgumentsContext {
        schema,
        field_arguments,
        variables,
    };

    ExtensionDirectiveArgumentsQueryView { ctx, directive }
}

pub(crate) fn create_extension_directive_response_view<'ctx, 'resp>(
    ctx: ArgumentsContext<'ctx>,
    directive: ExtensionDirective<'ctx>,
    response_objects_view: ResponseObjectsView<'resp>,
) -> ExtensionDirectiveArgumentsResponseObjectsView<'resp>
where
    'ctx: 'resp,
{
    let arguments = directive
        .arguments_with_stage(|stage| matches!(stage, InjectionStage::Response))
        .collect::<Vec<_>>();

    ExtensionDirectiveArgumentsResponseObjectsView {
        ctx,
        arguments,
        response_objects_view,
    }
}
