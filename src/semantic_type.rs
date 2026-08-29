#![allow(dead_code)]

// ALGOL26 Semantic Type - used throughout the IR
// Not String, but a proper type hierarchy

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticType {
    Int,
    Float,
    String,
    Bool,
    List(Box<SemanticType>),
    Void,
    Unknown,
    Option(Box<SemanticType>),
    Channel(Box<SemanticType>),
    Result {
        ok: Box<SemanticType>,
        error: Box<SemanticType>,
    },
    Pointer(Box<SemanticType>),
}

impl SemanticType {
    pub fn name(&self) -> String {
        match self {
            SemanticType::Int => "Int".to_string(),
            SemanticType::Float => "Float".to_string(),
            SemanticType::String => "String".to_string(),
            SemanticType::Bool => "Bool".to_string(),
            SemanticType::List(t) => format!("List<{}>", t.name()),
            SemanticType::Void => "Void".to_string(),
            SemanticType::Unknown => "Unknown".to_string(),
            SemanticType::Option(t) => format!("Option<{}>", t.name()),
            SemanticType::Result { ok, error } => format!("Result<{}, {}>", ok.name(), error.name()),
            SemanticType::Channel(t) => format!("Channel<{}>", t.name()),
            SemanticType::Pointer(t) => format!("*{}", t.name()),
        }
    }
    
    pub fn from_str(s: &str) -> Self {
        match s {
            "int" => SemanticType::Int,
            "float" => SemanticType::Float,
            "string" => SemanticType::String,
            "bool" => SemanticType::Bool,
            "void" => SemanticType::Void,
            "*float" => SemanticType::Pointer(Box::new(SemanticType::Float)),
            "*int" => SemanticType::Pointer(Box::new(SemanticType::Int)),
            "*string" => SemanticType::Pointer(Box::new(SemanticType::String)),
            "*bool" => SemanticType::Pointer(Box::new(SemanticType::Bool)),
            _ => SemanticType::Unknown,
        }
    }
}
