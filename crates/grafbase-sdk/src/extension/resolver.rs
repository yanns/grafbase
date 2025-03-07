use crate::{
    component::AnyExtension,
    types::{Directive, FieldDefinition, FieldInputs, FieldOutput},
    wit::{Error, SharedContext},
};

use super::Extension;

/// A trait that extends `Extension` and provides functionality for resolving fields.
///
/// Implementors of this trait are expected to provide a method to resolve field values based on
/// the given context, directive, and inputs. This is typically used in scenarios where field
/// resolution logic needs to be encapsulated within a resolver object, allowing for modular
/// and reusable code design.
pub trait Resolver: Extension {
    /// Resolves a field value based on the given context, directive, definition, and inputs.
    ///
    /// # Arguments
    ///
    /// * `context` - The shared context containing runtime information
    /// * `directive` - The directive associated with this field resolution
    /// * `definition` - The field definition containing metadata
    /// * `inputs` - The input values provided for this field
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing either the resolved `FieldOutput` value or an `Error`
    fn resolve_field(
        &mut self,
        context: SharedContext,
        directive: Directive,
        definition: FieldDefinition,
        inputs: FieldInputs,
    ) -> Result<FieldOutput, Error>;

    /// Resolves a subscription field by setting up a subscription handler.
    ///
    /// # Arguments
    ///
    /// * `context` - The shared context containing runtime information
    /// * `directive` - The directive associated with this subscription field
    /// * `definition` - The field definition containing metadata about the subscription
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing either a boxed `Subscriber` implementation or an `Error`
    fn resolve_subscription(
        &mut self,
        context: SharedContext,
        directive: Directive,
        definition: FieldDefinition,
    ) -> Result<Box<dyn Subscription>, Error>;
}

/// A trait for consuming field outputs from streams.
///
/// This trait provides an abstraction over different implementations
/// of subscriptions to field output streams. Implementors should handle
/// the details of their specific transport mechanism while providing a
/// consistent interface for consumers.
pub trait Subscription {
    /// Retrieves the next field output from the subscription.
    ///
    /// Returns:
    /// - `Ok(Some(FieldOutput))` if a field output was available
    /// - `Ok(None)` if the subscription has ended normally
    /// - `Err(Error)` if an error occurred while retrieving the next field output
    fn next(&mut self) -> Result<Option<FieldOutput>, Error>;
}

#[doc(hidden)]
pub fn register<T: Resolver>() {
    pub(super) struct Proxy<T: Resolver>(T);

    impl<T: Resolver> AnyExtension for Proxy<T> {
        fn resolve_field(
            &mut self,
            context: SharedContext,
            directive: Directive,
            definition: FieldDefinition,
            inputs: FieldInputs,
        ) -> Result<FieldOutput, Error> {
            Resolver::resolve_field(&mut self.0, context, directive, definition, inputs)
        }
        fn resolve_subscription(
            &mut self,
            context: SharedContext,
            directive: Directive,
            definition: FieldDefinition,
        ) -> Result<Box<dyn Subscription>, Error> {
            Resolver::resolve_subscription(&mut self.0, context, directive, definition)
        }
    }
    crate::component::register_extension(Box::new(|schema_directives, config| {
        <T as Extension>::new(schema_directives, config)
            .map(|extension| Box::new(Proxy(extension)) as Box<dyn AnyExtension>)
    }))
}
