//! A schema model built from a standard introspection response,
//! it's used to list root operations (queries/mutations/subscriptions)
//! and generate a starter query + variables skeleton for one of them

use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationKind {
    Query,
    Mutation,
    Subscription,
}

impl OperationKind {
    pub const ALL: [Self; 3] = [Self::Query, Self::Mutation, Self::Subscription];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Query => "Queries",
            Self::Mutation => "Mutations",
            Self::Subscription => "Subscriptions",
        }
    }

    fn keyword(&self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SchemaArg {
    pub name: String,
    pub type_str: String,
    pub named_type: String,
}

#[derive(Debug, Clone)]
pub struct SchemaField {
    pub name: String,
    pub args: Vec<SchemaArg>,
    pub named_type: String,
}

#[derive(Debug, Clone)]
struct SchemaType {
    kind: String,
    fields: Vec<SchemaField>,
}

#[derive(Debug, Clone, Default)]
pub struct Schema {
    query_type: Option<String>,
    mutation_type: Option<String>,
    subscription_type: Option<String>,
    types: HashMap<String, SchemaType>,
}

fn type_ref_string(t: &Value) -> String {
    match t.get("kind").and_then(Value::as_str) {
        Some("NON_NULL") => format!(
            "{}!",
            type_ref_string(t.get("ofType").unwrap_or(&Value::Null))
        ),
        Some("LIST") => format!(
            "[{}]",
            type_ref_string(t.get("ofType").unwrap_or(&Value::Null))
        ),
        _ => t
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Unknown")
            .to_string(),
    }
}

fn named_type(t: &Value) -> String {
    match t.get("kind").and_then(Value::as_str) {
        Some("NON_NULL") | Some("LIST") => named_type(t.get("ofType").unwrap_or(&Value::Null)),
        _ => t
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Unknown")
            .to_string(),
    }
}

fn parse_field(f: &Value) -> SchemaField {
    let type_val = f.get("type").cloned().unwrap_or(Value::Null);
    let args = f
        .get("args")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|a| {
                    let a_type = a.get("type").cloned().unwrap_or(Value::Null);
                    SchemaArg {
                        name: a
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        type_str: type_ref_string(&a_type),
                        named_type: named_type(&a_type),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    SchemaField {
        name: f
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        args,
        named_type: named_type(&type_val),
    }
}

impl Schema {
    /// Parses the `data.__schema` shape from a standard introspection query
    /// response body. Returns `None` if the body isn't a schema response.
    pub fn parse(introspection_body: &str) -> Option<Schema> {
        let json: Value = serde_json::from_str(introspection_body).ok()?;
        let schema_val = json.pointer("/data/__schema")?;

        let query_type = schema_val
            .pointer("/queryType/name")
            .and_then(Value::as_str)
            .map(String::from);
        let mutation_type = schema_val
            .pointer("/mutationType/name")
            .and_then(Value::as_str)
            .map(String::from);
        let subscription_type = schema_val
            .pointer("/subscriptionType/name")
            .and_then(Value::as_str)
            .map(String::from);

        let mut types = HashMap::new();
        if let Some(type_list) = schema_val.get("types").and_then(Value::as_array) {
            for t in type_list {
                let name = t.get("name").and_then(Value::as_str).unwrap_or("");
                if name.is_empty() || name.starts_with("__") {
                    continue;
                }
                let kind = t
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let fields = t
                    .get("fields")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().map(parse_field).collect())
                    .unwrap_or_default();
                types.insert(name.to_string(), SchemaType { kind, fields });
            }
        }

        Some(Schema {
            query_type,
            mutation_type,
            subscription_type,
            types,
        })
    }

    /// The root fields available for `kind` (e.g. every top-level query),
    /// empty if the schema doesn't define that operation kind at all.
    pub fn operations(&self, kind: OperationKind) -> Vec<&SchemaField> {
        let type_name = match kind {
            OperationKind::Query => &self.query_type,
            OperationKind::Mutation => &self.mutation_type,
            OperationKind::Subscription => &self.subscription_type,
        };
        type_name
            .as_ref()
            .and_then(|n| self.types.get(n))
            .map(|t| t.fields.iter().collect())
            .unwrap_or_default()
    }

    /// Whether `type_name` has its own selectable fields (so a field
    /// returning it needs a `{ ... }` selection set, and can be drilled
    /// into in the explorer tree).
    pub fn is_selectable(&self, type_name: &str) -> bool {
        self.types
            .get(type_name)
            .is_some_and(|t| t.kind == "OBJECT" || t.kind == "INTERFACE")
    }

    /// One level of checkbox-tree children for `type_name`: every field on
    /// that type, with scalar/enum fields pre-checked (an immediately
    /// runnable default selection) and object fields left unchecked and
    /// collapsed until the user drills into them
    pub fn selection_children(&self, type_name: &str) -> Vec<SelectionNode> {
        let Some(t) = self.types.get(type_name) else {
            return Vec::new();
        };
        t.fields
            .iter()
            .map(|f| {
                let is_object = self.is_selectable(&f.named_type);
                SelectionNode {
                    field_name: f.name.clone(),
                    named_type: f.named_type.clone(),
                    is_object,
                    checked: !is_object,
                    expanded: false,
                    children: Vec::new(),
                }
            })
            .collect()
    }

    /// Builds `(query, variables_json)` for `field` from the explorer tree's
    /// current checked/expanded state: one variable + argument per input
    /// arg (as before), and a selection set that mirrors exactly what's
    /// checked in `tree` rather than a fixed depth.
    pub fn build_query_from_selection(
        &self,
        kind: OperationKind,
        field: &SchemaField,
        tree: &[SelectionNode],
    ) -> (String, String) {
        let mut var_defs = Vec::new();
        let mut call_args = Vec::new();
        let mut variables = serde_json::Map::new();

        for arg in &field.args {
            var_defs.push(format!("${}: {}", arg.name, arg.type_str));
            call_args.push(format!("{0}: ${0}", arg.name));
            variables.insert(arg.name.clone(), default_scalar_value(&arg.named_type));
        }

        let args_str = if call_args.is_empty() {
            String::new()
        } else {
            format!("({})", call_args.join(", "))
        };
        let var_defs_str = if var_defs.is_empty() {
            String::new()
        } else {
            format!("({})", var_defs.join(", "))
        };

        let selection = serialize_selection(tree, 1);
        let selection_str = if self.is_selectable(&field.named_type) {
            if selection.is_empty() {
                " {\n    \n  }".to_string()
            } else {
                format!(" {{\n{selection}\n  }}")
            }
        } else {
            String::new()
        };

        let operation_name = pascal_case(&field.name);
        let query = format!(
            "{} {operation_name}{var_defs_str} {{\n  {}{args_str}{selection_str}\n}}",
            kind.keyword(),
            field.name,
        );
        let variables_json = serde_json::to_string_pretty(&Value::Object(variables))
            .unwrap_or_else(|_| "{}".to_string());

        (query, variables_json)
    }
}

/// A node in the interactive selection tree shown next to an operation: a
/// checkbox to include the field, and (for object-returning fields) an
/// expand toggle that lazily fetches its own children on first expand.
#[derive(Debug, Clone)]
pub struct SelectionNode {
    pub field_name: String,
    pub named_type: String,
    pub is_object: bool,
    pub checked: bool,
    pub expanded: bool,
    pub children: Vec<SelectionNode>,
}

/// Finds the node at `path` (a sequence of child indices, root-first).
pub fn find_node_mut<'a>(
    tree: &'a mut [SelectionNode],
    path: &[usize],
) -> Option<&'a mut SelectionNode> {
    let (first, rest) = path.split_first()?;
    let node = tree.get_mut(*first)?;
    if rest.is_empty() {
        Some(node)
    } else {
        find_node_mut(&mut node.children, rest)
    }
}

/// Serializes every checked node into a `field1\nfield2 {\n  nested\n}`-style
/// body; a checked-but-still-collapsed object field is skipped, since it has
/// no children to select from yet.
fn serialize_selection(nodes: &[SelectionNode], depth: usize) -> String {
    let indent = "  ".repeat(depth + 1);
    let mut lines = Vec::new();
    for node in nodes {
        if !node.checked {
            continue;
        }
        if node.is_object {
            if !node.expanded {
                continue;
            }
            let inner = serialize_selection(&node.children, depth + 1);
            if inner.is_empty() {
                continue;
            }
            lines.push(format!(
                "{indent}{} {{\n{inner}\n{indent}}}",
                node.field_name
            ));
        } else {
            lines.push(format!("{indent}{}", node.field_name));
        }
    }
    lines.join("\n")
}

fn default_scalar_value(named_type: &str) -> Value {
    match named_type {
        "Int" | "Float" => Value::Number(0.into()),
        "Boolean" => Value::Bool(false),
        _ => Value::String(String::new()),
    }
}

fn pascal_case(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
