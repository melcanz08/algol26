// src/ffi/c.rs - HARDENED
// Complete C ABI types with full type checking

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum CType {
    CVoid,
    CBool,
    CChar,
    CUChar,
    CShort,
    CUShort,
    CInt,
    CUInt,
    CLong,
    CULong,
    CLongLong,
    CULongLong,
    CFloat,
    CDouble,
    CString,
    CPointer(Box<CType>),
    CConstPointer(Box<CType>),
    CStruct(String),
    CUnion(String),
    CEnum(String),
    CArray(Box<CType>, usize),
    CFunctionPointer(Box<CFunctionSignature>),
    CSizeT,
    CSSizeT,
    CIntPtrT,
    CUIntPtrT,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CFunctionSignature {
    pub params: Vec<CType>,
    pub return_type: Box<CType>,
    pub variadic: bool,
}

impl CFunctionSignature {
    pub fn new(return_type: CType) -> Self {
        CFunctionSignature {
            params: Vec::new(),
            return_type: Box::new(return_type),
            variadic: false,
        }
    }

    pub fn with_params(mut self, params: Vec<CType>) -> Self {
        self.params = params;
        self
    }

    pub fn with_variadic(mut self, variadic: bool) -> Self {
        self.variadic = variadic;
        self
    }
}

#[derive(Clone, Debug)]
pub struct FFIInfo {
    pub abi: String,
    pub library: String,
    pub symbol_name: Option<String>,
    pub param_types: Vec<CType>,
    pub return_type: CType,
    pub variadic: bool,
    pub link_kind: LinkKind,
    pub safety_checks: Vec<SafetyCheck>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkKind {
    Dynamic,
    Static,
    Framework,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SafetyCheck {
    NullCheck,          // Check for null pointers
    BoundsCheck(usize), // Check bounds for arrays
    MemoryOwnership,    // Track memory ownership
    TypeValidation,     // Validate type compatibility
}

impl Default for FFIInfo {
    fn default() -> Self {
        FFIInfo {
            abi: "C".to_string(),
            library: String::new(),
            symbol_name: None,
            param_types: Vec::new(),
            return_type: CType::CVoid,
            variadic: false,
            link_kind: LinkKind::Dynamic,
            safety_checks: Vec::new(),
        }
    }
}

impl FFIInfo {
    pub fn get_symbol_name<'a>(&'a self, fallback: &'a str) -> &'a str {
        match &self.symbol_name {
            Some(name) => name.as_str(),
            None => fallback,
        }
    }

    pub fn get_library_filename(&self) -> Option<String> {
        if self.library.is_empty() {
            return None;
        }

        let lib = &self.library;

        #[cfg(target_os = "linux")]
        {
            Some(format!("lib{}.so", lib))
        }

        #[cfg(target_os = "macos")]
        {
            Some(format!("lib{}.dylib", lib))
        }

        #[cfg(target_os = "windows")]
        {
            Some(format!("{}.dll", lib))
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            None
        }
    }

    pub fn validate_types(&self, algol_types: &[crate::common::types::Type]) -> Result<(), String> {
        if self.param_types.len() != algol_types.len() {
            return Err(format!(
                "FFI function expects {} parameters but got {}",
                self.param_types.len(),
                algol_types.len()
            ));
        }

        for (i, (c_type, algol_type)) in self.param_types.iter().zip(algol_types).enumerate() {
            if !Self::types_compatible(c_type, algol_type) {
                return Err(format!(
                    "FFI parameter {} type mismatch: C type {} vs Algol26 type {}",
                    i, c_type, algol_type
                ));
            }
        }

        Ok(())
    }

    fn types_compatible(c_type: &CType, algol_type: &crate::common::types::Type) -> bool {
        match (c_type, algol_type) {
            (CType::CInt, crate::common::types::Type::Int) => true,
            (CType::CLong, crate::common::types::Type::Int) => true,
            (CType::CLongLong, crate::common::types::Type::Int) => true,
            (CType::CFloat, crate::common::types::Type::Float) => true,
            (CType::CDouble, crate::common::types::Type::Float) => true,
            (CType::CBool, crate::common::types::Type::Bool) => true,
            (CType::CString, crate::common::types::Type::String) => true,
            (CType::CVoid, crate::common::types::Type::Void) => true,
            (CType::CPointer(_), crate::common::types::Type::Ptr) => true,
            (CType::CPointer(_), crate::common::types::Type::Pointer(_)) => true,
            (CType::CConstPointer(_), crate::common::types::Type::Ptr) => true,
            _ => false,
        }
    }
}

impl fmt::Display for CType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CType::CVoid => write!(f, "void"),
            CType::CBool => write!(f, "bool"),
            CType::CChar => write!(f, "char"),
            CType::CUChar => write!(f, "unsigned char"),
            CType::CShort => write!(f, "short"),
            CType::CUShort => write!(f, "unsigned short"),
            CType::CInt => write!(f, "int"),
            CType::CUInt => write!(f, "unsigned int"),
            CType::CLong => write!(f, "long"),
            CType::CULong => write!(f, "unsigned long"),
            CType::CLongLong => write!(f, "long long"),
            CType::CULongLong => write!(f, "unsigned long long"),
            CType::CFloat => write!(f, "float"),
            CType::CDouble => write!(f, "double"),
            CType::CString => write!(f, "char*"),
            CType::CPointer(t) => write!(f, "{}*", t),
            CType::CConstPointer(t) => write!(f, "const {}*", t),
            CType::CStruct(name) => write!(f, "struct {}", name),
            CType::CUnion(name) => write!(f, "union {}", name),
            CType::CEnum(name) => write!(f, "enum {}", name),
            CType::CArray(t, n) => write!(f, "{}[{}]", t, n),
            CType::CFunctionPointer(sig) => write!(
                f,
                "fn({}) -> {}",
                sig.params
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                sig.return_type
            ),
            CType::CSizeT => write!(f, "size_t"),
            CType::CSSizeT => write!(f, "ssize_t"),
            CType::CIntPtrT => write!(f, "intptr_t"),
            CType::CUIntPtrT => write!(f, "uintptr_t"),
        }
    }
}

pub fn algol26_to_c_type(type_name: &str) -> Option<CType> {
    match type_name {
        "Int" | "int" => Some(CType::CLong),
        "Float" | "float" => Some(CType::CDouble),
        "Bool" | "bool" => Some(CType::CBool),
        "String" | "string" => Some(CType::CString),
        "Void" | "void" => Some(CType::CVoid),
        "Ptr" | "ptr" => Some(CType::CPointer(Box::new(CType::CVoid))),
        _ => None,
    }
}

pub fn c_to_algol26_type(c_type: &CType) -> Option<&'static str> {
    match c_type {
        CType::CVoid => Some("Void"),
        CType::CBool => Some("Bool"),
        CType::CChar
        | CType::CUChar
        | CType::CShort
        | CType::CUShort
        | CType::CInt
        | CType::CUInt
        | CType::CLong
        | CType::CULong
        | CType::CLongLong
        | CType::CULongLong => Some("Int"),
        CType::CFloat | CType::CDouble => Some("Float"),
        CType::CString => Some("String"),
        CType::CPointer(_) | CType::CConstPointer(_) => Some("Ptr"),
        CType::CSizeT | CType::CSSizeT | CType::CIntPtrT | CType::CUIntPtrT => Some("Int"),
        _ => None,
    }
}
