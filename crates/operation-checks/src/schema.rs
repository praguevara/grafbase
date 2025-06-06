mod async_graphql;
mod wrapping_types;

pub(crate) use self::{async_graphql::extract_type, wrapping_types::*};

use std::collections::HashSet;

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub(crate) usize);

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArgumentId(pub(crate) usize);

/// A GraphQL schema to run operation checks against.
#[derive(Debug)]
pub struct Schema {
    // Invariant: sorted.
    pub(crate) fields: Vec<SchemaField>,

    // Invariant: sorted
    pub(crate) field_arguments: Vec<FieldArgument>,

    /// (implementer, interface)
    /// Invariant: sorted
    pub(crate) interface_implementations: Vec<(String, String)>,

    pub(crate) input_objects: HashSet<String>,

    pub(crate) query_type_name: String,
    pub(crate) mutation_type_name: String,
    pub(crate) subscription_type_name: String,
}

impl Schema {
    pub(crate) fn find_field(&self, type_name: &str, field_name: &str) -> Option<FieldId> {
        self.fields
            .binary_search_by_key(
                &(type_name, field_name),
                |SchemaField {
                     type_name, field_name, ..
                 }| { (type_name, field_name) },
            )
            .map(FieldId)
            .ok()
    }

    /// Takes a path like "type_name.field_name.argument_name".
    pub(crate) fn find_argument(
        &self,
        path @ (_type_name, _field_name, _arg_name): (&str, &str, &str),
    ) -> Option<ArgumentId> {
        self.field_arguments
            .binary_search_by_key(&path, FieldArgument::sort_key)
            .map(ArgumentId)
            .ok()
    }

    pub(crate) fn iter_fields<'a: 'b, 'b>(&'a self, type_name: &'b str) -> impl Iterator<Item = &'a SchemaField> + 'b {
        let start = self
            .fields
            .partition_point(|SchemaField { type_name: other, .. }| other.as_str() < type_name);

        self.fields[start..].iter().take_while(
            move |SchemaField {
                      type_name: other_type_name,
                      ..
                  }| type_name == other_type_name,
        )
    }

    pub(crate) fn iter_field_arguments(&self, field_id: FieldId) -> impl Iterator<Item = (ArgumentId, &FieldArgument)> {
        let field = &self[field_id];
        let start = self.field_arguments.partition_point(
            |FieldArgument {
                 type_name, field_name, ..
             }| { type_name < &field.type_name && field_name < &field.field_name },
        );

        self.field_arguments[start..]
            .iter()
            .take_while(
                |FieldArgument {
                     type_name, field_name, ..
                 }| { type_name == &field.type_name && field_name == &field.field_name },
            )
            .enumerate()
            .map(move |(idx, arg)| (ArgumentId(idx + start), arg))
    }

    pub(crate) fn iter_interface_implementations<'a>(
        &'a self,
        type_name: &'a str,
    ) -> impl Iterator<Item = &'a str> + 'a {
        let start = self
            .interface_implementations
            .partition_point(|(implementer, _interface)| implementer.as_str() < type_name);

        self.interface_implementations[start..]
            .iter()
            .take_while(move |(implementer, _interface)| implementer == type_name)
            .map(|(_implementer, interface)| interface.as_str())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct SchemaField {
    pub(crate) type_name: String,
    pub(crate) field_name: String,
    /// The type of the field without any wrapping type (! and []).
    pub(crate) base_type: String,
    pub(crate) wrappers: WrappingTypes,
}

impl SchemaField {
    pub(crate) fn render_type(&self) -> impl std::fmt::Display + '_ {
        self.wrappers.render(&self.base_type)
    }

    pub(crate) fn is_required(&self) -> bool {
        self.wrappers.is_nonnull()
    }
}

impl PartialOrd for SchemaField {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(Ord::cmp(self, other))
    }
}

impl Ord for SchemaField {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.type_name
            .cmp(&other.type_name)
            .then_with(|| self.field_name.cmp(&other.field_name))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FieldArgument {
    pub(crate) type_name: String,
    pub(crate) field_name: String,
    pub(crate) argument_name: String,
    /// The type of the field without any wrapping type (! and []).
    pub(crate) base_type: String,
    pub(crate) wrappers: WrappingTypes,
    pub(crate) has_default: bool,
}

impl FieldArgument {
    fn sort_key(&self) -> (&str, &str, &str) {
        (&self.type_name, &self.field_name, &self.argument_name)
    }

    pub fn is_required(&self) -> bool {
        self.wrappers.is_nonnull()
    }

    pub(crate) fn is_required_without_default_value(&self) -> bool {
        self.is_required() && !self.has_default
    }
}

impl Ord for FieldArgument {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (&self.type_name, &self.field_name, &self.argument_name).cmp(&(
            &other.type_name,
            &other.field_name,
            &other.argument_name,
        ))
    }
}

impl PartialOrd for FieldArgument {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(Ord::cmp(self, other))
    }
}

impl std::ops::Index<FieldId> for Schema {
    type Output = SchemaField;

    fn index(&self, index: FieldId) -> &Self::Output {
        &self.fields[index.0]
    }
}

impl std::ops::Index<ArgumentId> for Schema {
    type Output = FieldArgument;

    fn index(&self, index: ArgumentId) -> &Self::Output {
        &self.field_arguments[index.0]
    }
}
