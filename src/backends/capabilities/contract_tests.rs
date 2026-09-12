// src/backends/capabilities/contract.rs

use super::*;
use super::scan::scan_call_name;
use std::collections::HashSet;

#[cfg(test)]
mod contract_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn classified_builtins_are_exactly_the_unlowered_set() {
        // Every name classified here must be absent from the LLVM
        // backend's compile_builtin_value. That check lives in a
        // separate crate, so this test just pins the classification
        // itself — a change to scan_call_name has to notice this test.
        let cases: &[(&str, Option<Feature>)] = &[
            ("String.concat", Some(Feature::StringFunctions)),
            ("String.substring", Some(Feature::StringFunctions)),
            ("String.to_upper", Some(Feature::StringFunctions)),
            ("String.to_lower", Some(Feature::StringFunctions)),
            ("String.length", None),
            ("String.len", None),
            ("File.read", Some(Feature::FileFunctions)),
            ("File.write", Some(Feature::FileFunctions)),
            ("File.append", Some(Feature::FileFunctions)),
            ("List.sum", Some(Feature::ListAggregates)),
            ("List.max", Some(Feature::ListAggregates)),
            ("List.min", Some(Feature::ListAggregates)),
            ("List.length", None),
            ("Math.sqrt", None),
            ("print", None),
        ];

        for (name, expected) in cases {
            let mut used = HashSet::new();
            scan_call_name(name, &mut used);
            match expected {
                Some(f) => assert!(
                    used.contains(f),
                    "'{name}' should be classified as {f:?}, got {used:?}"
                ),
                None => assert!(
                    used.is_empty(),
                    "'{name}' should not be classified, got {used:?}"
                ),
            }
        }
    }
}