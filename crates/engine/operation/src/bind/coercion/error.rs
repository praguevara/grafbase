use crate::Location;

#[derive(Debug, thiserror::Error)]
pub enum InputValueError {
    #[error("Exactly one field must be provided for {name} with @oneOf: {message}{path}")]
    ExactlyOneFIeldMustBePresentForOneOfInputObjects {
        name: String,
        path: String,
        message: String,
        location: Location,
    },
    #[error("Found a null where we expected a {expected}{path}")]
    UnexpectedNull {
        expected: String,
        path: String,
        location: Location,
    },
    #[error("Found a {actual} value where we expected a {expected}{path}")]
    MissingList {
        actual: ValueKind,
        expected: String,
        path: String,
        location: Location,
    },
    #[error("Found a {actual} value where we expected a '{name}' input object{path}")]
    MissingObject {
        name: String,
        actual: ValueKind,
        path: String,
        location: Location,
    },
    #[error("Found a {actual} value where we expected a {expected} scalar{path}")]
    IncorrectScalarType {
        actual: ValueKind,
        expected: String,
        path: String,
        location: Location,
    },
    #[error("Found value {actual} which cannot be coerced into a {expected} scalar{path}")]
    IncorrectScalarValue {
        actual: String,
        expected: String,
        path: String,
        location: Location,
    },
    #[error("Found a {actual} value where we expected a {enum} enum value{path}")]
    IncorrectEnumValueType {
        r#enum: String,
        actual: ValueKind,
        path: String,
        location: Location,
    },
    #[error("Unknown enum value '{value}' for enum {enum}{path}")]
    UnknownEnumValue {
        r#enum: String,
        value: String,
        path: String,
        location: Location,
    },
    #[error(
        "Variable ${name} doesn't have the right type. Declared as '{variable_ty}' but used as '{actual_ty}'{path}"
    )]
    IncorrectVariableType {
        name: String,
        variable_ty: String,
        actual_ty: String,
        location: Location,
        path: String,
    },
    #[error("Input object {input_object} does not have a field named '{name}'{path}")]
    UnknownInputField {
        input_object: String,
        name: String,
        location: Location,
        path: String,
    },
    #[error("Unknown variable ${name}{path}")]
    UnknownVariable {
        name: String,
        location: Location,
        path: String,
    },
    #[error("Variable ${name} default value relies on another variable{path}")]
    VariableDefaultValueReliesOnAnotherVariable {
        name: String,
        location: Location,
        path: String,
    },
}

impl InputValueError {
    pub(crate) fn location(&self) -> Location {
        match self {
            InputValueError::UnexpectedNull { location, .. }
            | InputValueError::MissingList { location, .. }
            | InputValueError::MissingObject { location, .. }
            | InputValueError::IncorrectScalarType { location, .. }
            | InputValueError::IncorrectScalarValue { location, .. }
            | InputValueError::IncorrectEnumValueType { location, .. }
            | InputValueError::UnknownVariable { location, .. }
            | InputValueError::IncorrectVariableType { location, .. }
            | InputValueError::UnknownInputField { location, .. }
            | InputValueError::VariableDefaultValueReliesOnAnotherVariable { location, .. }
            | InputValueError::UnknownEnumValue { location, .. }
            | InputValueError::ExactlyOneFIeldMustBePresentForOneOfInputObjects { location, .. } => *location,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::Display)]
pub enum ValueKind {
    String,
    Integer,
    Enum,
    Float,
    Object,
    Boolean,
    List,
    Null,
}

impl From<serde_json::Value> for ValueKind {
    fn from(value: serde_json::Value) -> Self {
        (&value).into()
    }
}

impl From<&serde_json::Value> for ValueKind {
    fn from(value: &serde_json::Value) -> Self {
        use serde_json::Value;

        match value {
            Value::Null => ValueKind::Null,
            Value::Number(number) if number.is_f64() => ValueKind::Float,
            Value::Number(_) => ValueKind::Integer,
            Value::String(_) => ValueKind::String,
            Value::Object(_) => ValueKind::Object,
            Value::Bool(_) => ValueKind::Boolean,
            Value::Array(_) => ValueKind::List,
        }
    }
}

impl From<cynic_parser::ConstValue<'_>> for ValueKind {
    fn from(value: cynic_parser::ConstValue<'_>) -> Self {
        cynic_parser::Value::from(value).into()
    }
}

impl From<cynic_parser::Value<'_>> for ValueKind {
    fn from(value: cynic_parser::Value<'_>) -> Self {
        use cynic_parser::Value;
        match value {
            Value::Null(_) => ValueKind::Null,
            Value::Float(_) => ValueKind::Float,
            Value::Int(_) => ValueKind::Integer,
            Value::String(_) => ValueKind::String,
            Value::Boolean(_) => ValueKind::Boolean,
            Value::Enum(_) => ValueKind::Enum,
            Value::List(_) => ValueKind::List,
            Value::Object(_) => ValueKind::Object,
            Value::Variable(_) => unreachable!(),
        }
    }
}
