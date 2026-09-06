// src/ffi.rs
use std::fmt;

/// C ABI types for FFI interop
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
    CString, // char* (null-terminated)
    CPointer(Box<CType>),
    CConstPointer(Box<CType>),
    CStruct(String),           // Named C struct
    CUnion(String),            // Named C union
    CEnum(String),             // Named C enum
    CArray(Box<CType>, usize), // Fixed-size array
    CFunctionPointer(Box<CFunctionSignature>),
    CSizeT,    // size_t
    CSSizeT,   // ssize_t
    CIntPtrT,  // intptr_t
    CUIntPtrT, // uintptr_t
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
}

/// FFI metadata for external functions
#[derive(Clone, Debug)]
pub struct FFIInfo {
    /// ABI specification: "C", "system", "stdcall", "fastcall", etc.
    pub abi: String,
    /// Library name without extension: "libc", "libm", "user32"
    pub library: String,
    /// Actual symbol name if different from Algol26 function name
    pub symbol_name: Option<String>,
    /// C types for parameters
    pub param_types: Vec<CType>,
    /// C return type
    pub return_type: CType,
    /// Whether this function is variadic (like printf)
    pub variadic: bool,
    /// Link kind: dynamic (default) or static
    pub link_kind: LinkKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkKind {
    Dynamic,
    Static,
    Framework, // macOS frameworks
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
        }
    }
}

impl FFIInfo {
    /// Get the actual symbol name for linking
    pub fn get_symbol_name<'a>(&'a self, fallback: &'a str) -> &'a str {
        match &self.symbol_name {
            Some(name) => name.as_str(),
            None => fallback,
        }
    }

    /// Get library name with platform-specific prefix/suffix
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

/// Convert Algol26 type names to C types
pub fn algol26_to_c_type(type_name: &str) -> Option<CType> {
    match type_name {
        "Int" => Some(CType::CLong),
        "Float" => Some(CType::CDouble),
        "Bool" => Some(CType::CBool),
        "String" => Some(CType::CString),
        "Void" => Some(CType::CVoid),
        _ => None,
    }
}

/// Convert C types to Algol26 type names
pub fn c_to_algol26_type(c_type: &CType) -> Option<&'static str> {
    match c_type {
        CType::CVoid => Some("Void"),
        CType::CBool => Some("Bool"),
        CType::CChar => Some("Int"),
        CType::CUChar => Some("Int"),
        CType::CShort => Some("Int"),
        CType::CUShort => Some("Int"),
        CType::CInt => Some("Int"),
        CType::CUInt => Some("Int"),
        CType::CLong => Some("Int"),
        CType::CULong => Some("Int"),
        CType::CLongLong => Some("Int"),
        CType::CULongLong => Some("Int"),
        CType::CFloat => Some("Float"),
        CType::CDouble => Some("Float"),
        CType::CString => Some("String"), // Requires safety checks
        CType::CPointer(_) => Some("Ptr"),
        CType::CConstPointer(_) => Some("Ptr"),
        CType::CSizeT => Some("Int"),
        CType::CSSizeT => Some("Int"),
        CType::CIntPtrT => Some("Int"),
        CType::CUIntPtrT => Some("Int"),
        _ => None,
    }
}

// Unit tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_type_display() {
        assert_eq!(CType::CInt.to_string(), "int");
        assert_eq!(CType::CPointer(Box::new(CType::CVoid)).to_string(), "void*");
        assert_eq!(CType::CString.to_string(), "char*");
    }

    #[test]
    fn test_ffi_library_filename() {
        let ffi = FFIInfo {
            library: "m".to_string(),
            ..Default::default()
        };

        let filename = ffi.get_library_filename();
        assert!(filename.is_some());

        #[cfg(target_os = "linux")]
        assert_eq!(
            filename.expect("ICE: unwrap - should be unreachable"),
            "libm.so"
        );

        #[cfg(target_os = "macos")]
        assert_eq!(
            filename.expect("ICE: unwrap - should be unreachable"),
            "libm.dylib"
        );

        #[cfg(target_os = "windows")]
        assert_eq!(
            filename.expect("ICE: unwrap - should be unreachable"),
            "m.dll"
        );
    }

    #[test]
    fn test_type_conversion() {
        assert_eq!(algol26_to_c_type("Int"), Some(CType::CLong));
        assert_eq!(algol26_to_c_type("Float"), Some(CType::CDouble));
        assert_eq!(c_to_algol26_type(&CType::CInt), Some("Int"));
    }
}
