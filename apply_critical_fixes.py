with open('src/semantic_builder.rs', 'r') as f:
    content = f.read()

# Fix 1: Match - always create merge block
content = content.replace(
    "        if all_unreachable {\n            FlowResult::Unreachable\n        } else {\n            func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new() });\n            FlowResult::Reachable(merge_id)\n        }",
    "        // Always create merge block since Switch references it\n        func.blocks.push(SemanticBlock { id: merge_id, instructions: Vec::new() });\n        if all_unreachable {\n            FlowResult::Unreachable\n        } else {\n            FlowResult::Reachable(merge_id)\n        }"
)

# Fix 2: Add coercion helper
content = content.replace(
    "    fn validate_call(",
    """    fn coerce_value(&self, value: TypedIRValue, target: &SemanticType) -> TypedIRValue {
        let value_type = value.type_of();
        if value_type != SemanticType::Unknown
            && *target != SemanticType::Unknown
            && value_type.can_coerce_to(target)
            && value_type != *target
        {
            TypedIRValue::Cast {
                value: Box::new(value),
                target_type: target.clone(),
            }
        } else {
            value
        }
    }
    
    fn validate_call("
)

# Fix 3: Apply coercion to function arguments
content = content.replace(
    "                let typed_args: Vec<TypedIRValue> = args.iter().map(|a| self.translate_expr(a)).collect();\n                let return_type = self.validate_call(name, &typed_args);\n                TypedIRValue::Call { function: name.clone(), args: typed_args, return_type }",
    "                let typed_args: Vec<TypedIRValue> = args.iter().map(|a| self.translate_expr(a)).collect();\n                let return_type = self.validate_call(name, &typed_args);\n                let coerced_args: Vec<TypedIRValue> = typed_args.iter().map(|a| self.coerce_value(a.clone(), &return_type)).collect();\n                TypedIRValue::Call { function: name.clone(), args: coerced_args, return_type }"
)

# Fix 4: Return type coercion with Cast
content = content.replace(
    "                if type_ != SemanticType::Unknown\n                    && func.return_type != SemanticType::Unknown\n                    && type_ != func.return_type\n                {\n                    self.diagnostics.push(format!(\n                        \"Return type mismatch in function '{}': expected {:?}, found {:?}\",\n                        func.name, func.return_type, type_\n                    ));\n                }\n                \n                SemanticInstruction::Return { value: typed_value, type_ }",
    "                let coerced_value = typed_value.map(|v| {\n                    self.coerce_value(v, &func.return_type)\n                });\n                let type_ = coerced_value.as_ref().map(|v| v.type_of()).unwrap_or(SemanticType::Void);\n                \n                if type_ != SemanticType::Unknown\n                    && func.return_type != SemanticType::Unknown\n                    && !type_.can_coerce_to(&func.return_type)\n                {\n                    self.diagnostics.push(format!(\n                        \"Return type mismatch in function '{}': expected {:?}, found {:?}\",\n                        func.name, func.return_type, type_\n                    ));\n                }\n                \n                SemanticInstruction::Return { value: coerced_value, type_ }"
)

# Fix 5: Missing return detection
content = content.replace(
    "            let _ = self.translate_block(&mut program, &mut semantic_func, entry_id, &func.body);\n            \n            self.pop_scope();\n            program.functions.push(semantic_func);",
    "            let flow = self.translate_block(&mut program, &mut semantic_func, entry_id, &func.body);\n            \n            // Missing return detection\n            if func.return_type.as_deref().map(SemanticType::from_str).unwrap_or(SemanticType::Void) != SemanticType::Void\n                && flow.is_reachable()\n            {\n                self.diagnostics.push(format!(\n                    \"Function '{}' may reach end without returning a value\",\n                    func.name\n                ));\n            }\n            \n            self.pop_scope();\n            program.functions.push(semantic_func);"
)

with open('src/semantic_builder.rs', 'w') as f:
    f.write(content)

print("✅ Applied critical fixes")
