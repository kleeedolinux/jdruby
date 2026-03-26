//! Utility functions for LLVM IR generation.

/// Sanitize a Ruby identifier for LLVM IR.
pub fn sanitize_name(name: &str) -> String {
    name.replace("::", "__")
        .replace('#', "__")
        .replace('<', "_")
        .replace('>', "_")
        .replace('?', "_q")
        .replace('!', "_b")
        .replace('.', "_")
        .replace('@', "_at_")
        .replace('$', "_global_")
        .replace(' ', "_")
}

/// Escape a string for LLVM IR constant representation.
pub fn llvm_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for byte in s.bytes() {
        match byte {
            b'\\' => out.push_str("\\5C"),
            b'"' => out.push_str("\\22"),
            b'\n' => out.push_str("\\0A"),
            b'\r' => out.push_str("\\0D"),
            b'\t' => out.push_str("\\09"),
            b'\0' => out.push_str("\\00"),
            0x20..=0x7E => out.push(byte as char),
            _ => out.push_str(&format!("\\{:02X}", byte)),
        }
    }
    out
}

/// Convert LLVM type to Ruby value type hint.
pub fn llvm_type_to_ruby_type(llvm_type: &str) -> &'static str {
    match llvm_type {
        "i64" => "Integer",
        "double" => "Float",
        "i1" => "Boolean",
        "i8*" => "String",
        _ => "Object",
    }
}
