use std::borrow::Cow;

use schema::EnumDefinitionId;
use serde::{
    Deserializer,
    de::{DeserializeSeed, Error, IgnoredAny, Unexpected, Visitor},
};
use walker::Walk;

use crate::{
    prepare::FieldShapeRecord,
    response::{GraphqlError, ResponseValue},
};

use super::SeedState;

pub(crate) struct EnumValueSeed<'ctx, 'parent, 'state> {
    pub state: &'state SeedState<'ctx, 'parent>,
    pub definition_id: EnumDefinitionId,
    pub field: &'ctx FieldShapeRecord,
    pub is_required: bool,
}

impl<'de> DeserializeSeed<'de> for EnumValueSeed<'_, '_, '_> {
    type Value = ResponseValue;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let Self {
            state,
            definition_id,
            field,
            is_required,
        } = self;
        match deserializer.deserialize_any(self)? {
            Ok(string_value) => {
                let mut resp = state.response.borrow_mut();
                match definition_id
                    .walk(state.schema)
                    .find_value_by_name(string_value.as_ref())
                {
                    // If inaccessible propagating an error without any message.
                    Some(enum_value) => {
                        let value = ResponseValue::StringId { id: enum_value.name_id };
                        if state.should_report_error_for(field) && enum_value.is_inaccessible() {
                            if is_required {
                                resp.propagate_null(&state.path());
                                Ok(value)
                            } else {
                                let id = resp.data.push_inaccessible_value(value);
                                Ok(ResponseValue::Inaccessible { id })
                            }
                        } else {
                            Ok(value)
                        }
                    }
                    None => {
                        tracing::error!("Unknown enum value: {string_value} at path '{}'", state.display_path());
                        if state.should_report_error_for(field) {
                            let path = state.path();
                            // If not required, we don't need to propagate as Unexpected is equivalent to
                            // null for users.
                            if is_required {
                                resp.propagate_null(&path);
                            }
                            resp.errors.push(
                                GraphqlError::invalid_subgraph_response()
                                    .with_path(path)
                                    .with_location(field.id.walk(state).location()),
                            );
                        }
                        Ok(ResponseValue::Unexpected)
                    }
                }
            }
            Err(value) => Ok(value),
        }
    }
}

impl EnumValueSeed<'_, '_, '_> {
    fn unexpected_type<'de>(&self, value: Unexpected<'_>) -> <Self as Visitor<'de>>::Value {
        tracing::error!(
            "invalid type: {}, expected an enum value at '{}'",
            value,
            self.state.display_path()
        );

        if self.state.should_report_error_for(self.field) {
            let mut resp = self.state.response.borrow_mut();
            let path = self.state.path();
            // If not required, we don't need to propagate as Unexpected is equivalent to
            // null for users.
            if self.is_required {
                resp.propagate_null(&path);
            }
            resp.errors.push(
                GraphqlError::invalid_subgraph_response()
                    .with_path(path)
                    .with_location(self.field.id.walk(self.state).location()),
            );
        }

        Err(ResponseValue::Unexpected)
    }
}

impl<'de> Visitor<'de> for EnumValueSeed<'_, '_, '_> {
    type Value = Result<Cow<'de, str>, ResponseValue>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(self.unexpected_type(Unexpected::Bool(v)))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(self.unexpected_type(Unexpected::Signed(v)))
    }

    fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(self.unexpected_type(Unexpected::Other(&format!("integer {v}"))))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(self.unexpected_type(Unexpected::Unsigned(v)))
    }

    fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(self.unexpected_type(Unexpected::Other(&format!("integer {v}"))))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(self.unexpected_type(Unexpected::Float(v)))
    }

    fn visit_char<E>(self, v: char) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(v.encode_utf8(&mut [0u8; 4]))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(Ok(Cow::Owned(v.to_string())))
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(Ok(Cow::Borrowed(v)))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(Ok(Cow::Owned(v)))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(self.unexpected_type(Unexpected::Bytes(v)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if self.is_required {
            Ok(self.unexpected_type(Unexpected::Option))
        } else {
            Ok(Err(ResponseValue::Null))
        }
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(self)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        if self.is_required {
            Ok(self.unexpected_type(Unexpected::Unit))
        } else {
            Ok(Err(ResponseValue::Null))
        }
    }

    fn visit_newtype_struct<D>(self, _: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        // newtype_struct are used by custom deserializers to indicate that an error happened, but
        // was already treated.
        Ok(Err(ResponseValue::Unexpected))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        // Try discarding the rest of the list, we might be able to use other parts of
        // the response.
        while seq.next_element::<IgnoredAny>()?.is_some() {}
        Ok(self.unexpected_type(Unexpected::Seq))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        // Try discarding the rest of the map, we might be able to use other parts of
        // the response.
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(self.unexpected_type(Unexpected::Map))
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::EnumAccess<'de>,
    {
        let _ = data.variant::<IgnoredAny>()?;
        Ok(self.unexpected_type(Unexpected::Enum))
    }
}
