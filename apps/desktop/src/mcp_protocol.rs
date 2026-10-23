//! MCP JSON-RPC metadata/discovery; only UI-owned operations cross the bounded transport queue.
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub const VERSIONS: &[&str] = &["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"];
pub const GUIDE: &str = include_str!("../../../docs/mcp.md");
fn info() -> Value {
    json!({"name":"meshcraft","version":env!("CARGO_PKG_VERSION")})
}
fn capabilities() -> Value {
    json!({"tools":{},"resources":{},"prompts":{}})
}
pub(super) fn error(id: Value, code: i64, message: &str, data: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message,"data":data}})
}
pub fn contract() -> Value {
    json!({"application":info(),"protocolVersions":VERSIONS,"transport":"Streamable HTTP, stateless JSON responses","endpoint":"http://127.0.0.1:<port>/mcp","defaultPort":8765,"guide":"meshcraft://guide","scriptingApi":"meshcraft://scripting/api","tools":tools(),"resources":resources(),"limits":{"requestBytes":8*1024*1024,"connections":8,"queue":16,"timeoutSeconds":15},"unsupported":["Groovy console","remote network binding","unsolicited SSE subscriptions"],"coordinates":{"up":"+Y","front":"+Z","unit":"internal model units; displayed length = internal length * unit_size, suffixed with unit_name; one grid step = grid_size * unit_size","angles":"degrees in SDK helpers; Math uses radians"},"completion":"Commands return accepted and completion state. Poll meshcraft_get_job for pending jobs. Revisions change only on committed edits. Tools and edits share the UI transaction path. Never repeat a timed-out mutation before inspecting state."})
}
fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn tool(name: &str, description: &str, input: Value, read_only: bool) -> Value {
    json!({"name":name,"description":description,"inputSchema":input,"annotations":{"readOnlyHint":read_only,"destructiveHint":!read_only,"openWorldHint":false}})
}
pub fn tools() -> Vec<Value> {
    vec![
        tool(
            "k3d_contract",
            "Start here: Meshcraft capabilities, coordinate conventions, discovery resources and command completion rules. Then read meshcraft://guide.",
            schema(json!({}), &[]),
            true,
        ),
        tool(
            "k3d_status",
            "Live application/server status: document revision/generation, active tool, edit context, selection and running server port.",
            schema(json!({}), &[]),
            true,
        ),
        tool(
            "k3d_list_commands",
            "Discover every registered command with its current enabled/disabled reason, typed JSON input schema, menu label and capability. Filter by category or case-insensitive text. Disabled commands are reported honestly.",
            schema(
                json!({"query":{"type":"string"},"category":{"type":"string"}}),
                &[],
            ),
            true,
        ),
        tool(
            "k3d_execute_command",
            "Execute a discovered command ID using its published input schema. Same transactions/capability checks as buttons. Tool activation does not finish geometry: use k3d_pointer_event world clicks and finish. For open/save supply a path to avoid interactive dialogs; inspect pending completion with meshcraft_get_job.",
            schema(
                json!({"commandId":{"type":"string"},"arguments":{"type":"object"}}),
                &["commandId"],
            ),
            false,
        ),
        tool(
            "k3d_pointer_event",
            "Operate the current tool in world model units. click picks a point, move sets the current direction/preview, text appends to the numeric buffer, backspace removes its last character (or undoes a point), finish presses Enter, cancel presses Escape. Numeric text accepts bare internal distances, unit-suffixed lengths (e.g. 30cm), relative plane coordinates 2,3 or world XYZ 2,3,4. Final rotation values are degrees (or deg/rad); scale values are factors. In copy mode x12 creates 12 new copies at the current increment; /12 creates 12 copies dividing the picked span/angle/scale, including its endpoint. A value and repetition may combine as 2m x12. Select entities explicitly with meshcraft_select.",
            schema(
                json!({"action":{"enum":["click","move","finish","cancel","text","backspace"]},"worldX":{"type":"number"},"worldY":{"type":"number"},"worldZ":{"type":"number"},"text":{"type":"string"}}),
                &["action"],
            ),
            false,
        ),
        tool(
            "meshcraft_scene",
            "Inspect scene objects/prototypes/materials/settings and exact occurrence paths. Paths are arrays of numeric layout-entry IDs; [] means root geometry. Optional object offset/limit bounds the response.",
            schema(
                json!({"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":256}}),
                &[],
            ),
            true,
        ),
        tool(
            "meshcraft_geometry",
            "Read an occurrence's current evaluated LOCAL geometry and world transform, including stable primitive IDs, authored/evaluated distinction, hotspots, script state and canonical architecture records/owner targets. Supply its numeric path from meshcraft_scene. Primitives are paginated; vertex records needed by returned primitives are included. Apply transform for world coordinates.",
            schema(
                json!({"path":{"type":"array","items":{"type":"integer","minimum":0}},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":1000}}),
                &["path"],
            ),
            true,
        ),
        tool(
            "meshcraft_select",
            "Replace/add/remove explicit selection targets returned by meshcraft_geometry. Targets have path:[ids], kind:occurrence|vertex|segment|triangle|semantic_handle, and id as a decimal string (or semantic ID). Inspect meshcraft://guide for exact examples. External/read-only geometry is rejected. An empty targets array clears selection.",
            schema(
                json!({"targets":{"type":"array","items":{"type":"object"}},"mode":{"enum":["replace","add","remove"]}}),
                &["targets"],
            ),
            false,
        ),
        tool(
            "meshcraft_script_api",
            "Discover the SAME JavaScript SDK signatures, descriptions and examples shown by Ctrl+Space in the object editor, including this/instance/model accessors and hook lifecycle. Optional query filters names/signatures/descriptions.",
            schema(json!({"query":{"type":"string"}}), &[]),
            true,
        ),
        tool(
            "meshcraft_ifc",
            "Get IFC4X3_ADD2 properties for the selected prototype or instances; catalog searches all 3,988 official standard properties/quantities; presets lists compact architectural subsets; preset adds missing fields with null (unset) values, preserving all existing properties, in one Undo transaction. preset requires definition_id and preset=wall|slab|door|window. set/delete edits one named property. Prototype properties are creation defaults copied into new instances. property has kind,dataType,value,unit,description. Omit paths for prototype defaults; use paths for selected instances. Presets do not assign an IFC entity class or invent quantities/performance values.",
            schema(
                json!({"action":{"enum":["get","catalog","presets","preset","set","delete"]},"preset":{"enum":["wall","slab","door","window"]},"prototype":{"type":"boolean"},"query":{"type":"string"},"definition_id":{"type":"integer"},"paths":{"type":"array","items":{"type":"array","items":{"type":"integer"}}},"set":{"type":"string"},"name":{"type":"string"},"property":{"type":"object"}}),
                &["action"],
            ),
            false,
        ),
        tool(
            "meshcraft_parameters",
            "Read Object Properties parameter forms for the current selection, or set an authored prototype/instance parameter in one undoable transaction. set requires definition_id,id,value; omit paths for shared prototype scope, or pass occurrence paths for instance scope. Values are strictly typed. Declare with $TEXT/$NUMBER/$CHECKBOX/$LIST/$HOTSPOT, using script api_version=2.",
            schema(
                json!({"action":{"enum":["get","set"]},"definition_id":{"type":"integer","minimum":0},"paths":{"type":"array","items":{"type":"array","items":{"type":"integer","minimum":0}}},"id":{"type":"string"},"value":{}}),
                &["action"],
            ),
            false,
        ),
        tool(
            "meshcraft_object_script",
            "Create a scripted object, read its full source, or replace a prototype's script in one undoable edit. create needs source and returns prototype/occurrence IDs; get/set need definition_id. Use api_version=2 for onAnimationFrame(params,model) and $ declarations. Legacy api_version=1 preserves the original instance argument. onCreate(instance,model) retains this as the instance. Editing a prototype affects its instances.",
            schema(
                json!({"action":{"enum":["create","get","set"]},"definition_id":{"type":"integer","minimum":0},"source":{"type":"string"},"api_version":{"type":"integer","enum":[1,2]}}),
                &["action"],
            ),
            false,
        ),
        tool(
            "meshcraft_run_script",
            "Run a ONE-SHOT script through the SAME JavaScript SDK as object scripts, off the UI thread. Default source_type=body accepts e.g. return cuboid(new Point(),new Point(3,2,1)); source_type=module defines onCreate/onAnimationFrame. this.geometry starts empty; model is an immutable live-scene snapshot, custom properties initialize this. Returns a job ID; poll meshcraft_get_job. commit=true (default) inserts one static object with emitted geometry/hotspots in ONE undo transaction, in the active editing context. Output coordinates are world model units plus optional position offset. commit=false evaluates without model changes. Rejects stale results after document/context changes; failures/cancel leave no geometry.",
            schema(
                json!({"source":{"type":"string"},"source_type":{"enum":["body","module"]},"api_version":{"type":"integer","enum":[1,2]},"properties":{"type":"object"},"name":{"type":"string"},"position":{"type":"array","items":{"type":"number"}},"commit":{"type":"boolean"}}),
                &["source"],
            ),
            false,
        ),
        tool(
            "meshcraft_cancel_script",
            "Cancel the current one-shot script; queued output cannot commit. Inspect its job result for cancellation. Stop/Restart or closing the app also cancels it.",
            schema(json!({}), &[]),
            false,
        ),
        tool(
            "meshcraft_capture_view",
            "Capture the current 3D viewport as a PNG image for visual verification. It includes model, helpers and tool preview, without UI panels or GPUI text labels. Default requires a freshly rendered frame; retry on rendering-pending errors, or set allow_stale=true for the latest available frame. Max width defaults to 1024. Returns image content plus frame/revision metadata.",
            schema(
                json!({"max_width":{"type":"integer","minimum":64,"maximum":2048},"allow_stale":{"type":"boolean"}}),
                &[],
            ),
            true,
        ),
        tool(
            "meshcraft_resolve_document",
            "Resolve an explicit pending New/Open unsaved-changes choice reported by k3d_status.pendingDialog. action=cancel retains the document; discard continues replacement; save saves before replacement (supply an absolute path when unnamed). Does not bypass the UI workflow or silently discard edits.",
            schema(
                json!({"action":{"enum":["save","discard","cancel"]},"path":{"type":"string"}}),
                &["action"],
            ),
            false,
        ),
        tool(
            "meshcraft_get_job",
            "Poll accepted asynchronous open/save/import/script jobs by ID. Returns pending/completed/failed and outputs; this does not re-execute the operation.",
            schema(json!({"job_id":{"type":"string"}}), &["job_id"]),
            true,
        ),
    ]
}
pub fn resources() -> Vec<Value> {
    [
        ("guide","Getting started","Model/selection conventions, exact schemas and end-to-end examples"),
        ("contract","Automation contract","Capabilities and transport/completion rules"),
        ("commands","Live command catalog","Commands, input schemas and current availability"),
        ("scene/summary","Live scene","Prototypes, instances, materials and model settings"),
        ("selection","Live selection","Exact selected targets and active editing context"),
        ("scripting/api","JavaScript API","Shared SDK signature/description/example catalog"),
        ("scripting/sdk","JavaScript SDK source","Exact injected SDK implementation; no network lookup needed"),
        ("logs","Recent MCP operations","Bounded action/status/error log; scripts and clipboard content omitted"),
    ].into_iter().map(|(path,name,description)| json!({"uri":format!("meshcraft://{path}"),"name":name,"description":description,"mimeType":if path=="guide" {"text/markdown"} else if path=="scripting/sdk" {"text/javascript"} else {"application/json"}})).collect()
}
fn validate(value: &Value, schema: &Value, field: &str) -> Result<(), String> {
    if let Some(options) = schema["enum"].as_array()
        && !options.contains(value)
    {
        return Err(format!("{field}: value is not in the published enum"));
    }
    if let Some(kind) = schema["type"].as_str() {
        let valid = match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            _ => true,
        };
        if !valid {
            return Err(format!("{field}: expected {kind}"));
        }
    }
    if let Some(number) = value.as_f64()
        && (schema["minimum"].as_f64().is_some_and(|n| number < n)
            || schema["maximum"].as_f64().is_some_and(|n| number > n))
    {
        return Err(format!("{field}: number outside published range"));
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema["required"].as_array() {
            for key in required {
                let key = key.as_str().unwrap_or("");
                if !object.contains_key(key) {
                    return Err(format!("{field}.{key} is required"));
                }
            }
        }
        for (key, value) in object {
            if let Some(child) = schema["properties"].get(key) {
                validate(value, child, &format!("{field}.{key}"))?;
            } else if schema["additionalProperties"] == false {
                return Err(format!("{field}: unknown argument {key}"));
            }
        }
    }
    if let Some(array) = value.as_array()
        && let Some(items) = schema.get("items")
    {
        for value in array {
            validate(value, items, field)?;
        }
    }
    Ok(())
}
pub(super) fn dispatch(
    body: Value,
    headers: &BTreeMap<String, String>,
    mut owner: impl FnMut(&str, Value, Value) -> Result<Value, String>,
) -> Result<(u16, Value), String> {
    let id = body.get("id").cloned().unwrap_or(Value::Null);
    if body["jsonrpc"] != "2.0"
        || !body.is_object()
        || !body["method"].is_string()
        || (!id.is_null() && !id.is_string() && !id.is_number())
    {
        return Ok((400, error(id, -32600, "Invalid Request", Value::Null)));
    }
    let method = body["method"].as_str().unwrap();
    let params = body.get("params").cloned().unwrap_or(json!({}));
    if !params.is_object() {
        return Ok((
            400,
            error(id, -32602, "params must be an object", Value::Null),
        ));
    }
    let version = headers
        .get("mcp-protocol-version")
        .map(String::as_str)
        .or_else(|| params["_meta"]["io.modelcontextprotocol/protocolVersion"].as_str())
        .unwrap_or("2025-03-26");
    let modern = version == "2026-07-28";
    if !VERSIONS.contains(&version) {
        return Ok((
            400,
            error(
                id,
                -32022,
                "Unsupported protocol version",
                json!({"requested":version,"supported":VERSIONS}),
            ),
        ));
    }
    if modern {
        let metadata = &params["_meta"];
        if headers.get("mcp-protocol-version").map(String::as_str) != Some(version)
            || metadata["io.modelcontextprotocol/protocolVersion"] != version
            || !metadata["io.modelcontextprotocol/clientInfo"]["name"].is_string()
            || !metadata["io.modelcontextprotocol/clientInfo"]["version"].is_string()
            || !metadata["io.modelcontextprotocol/clientCapabilities"].is_object()
            || headers.get("mcp-method").map(String::as_str) != Some(method)
            || matches!(method, "tools/call" | "resources/read" | "prompts/get")
                && headers.get("mcp-name").map(String::as_str)
                    != params
                        .get(if method == "resources/read" {
                            "uri"
                        } else {
                            "name"
                        })
                        .and_then(Value::as_str)
        {
            return Ok((
                400,
                error(
                    id,
                    -32602,
                    "Missing or mismatched MCP request metadata/headers",
                    Value::Null,
                ),
            ));
        }
    }
    if !body.as_object().unwrap().contains_key("id") {
        if matches!(
            method,
            "notifications/initialized" | "notifications/cancelled"
        ) {
            if method == "notifications/cancelled" {
                let _ = owner(method, params, Value::Null);
            }
            return Ok((202, Value::Null));
        }
        return Ok((
            400,
            error(Value::Null, -32600, "Unsupported notification", Value::Null),
        ));
    }
    let result = match method {
        "initialize" => {
            let requested = params["protocolVersion"].as_str().unwrap_or("2025-11-25");
            let negotiated = if VERSIONS[1..].contains(&requested) {
                requested
            } else {
                "2025-11-25"
            };
            Ok(
                json!({"protocolVersion":negotiated,"capabilities":capabilities(),"serverInfo":info(),"instructions":GUIDE}),
            )
        }
        "server/discover" => Ok(
            json!({"supportedVersions":VERSIONS,"capabilities":capabilities(),"_meta":{"io.modelcontextprotocol/serverInfo":info()},"instructions":GUIDE}),
        ),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools":tools()})),
        "resources/list" => Ok(json!({"resources":resources()})),
        "resources/templates/list" => Ok(json!({"resourceTemplates":[]})),
        "prompts/list" => Ok(
            json!({"prompts":[{"name":"meshcraft_getting_started","description":"Discover Meshcraft and construct a first model using shared commands or JavaScript helpers"}]}),
        ),
        "prompts/get" if params["name"] == "meshcraft_getting_started" => Ok(
            json!({"description":"Meshcraft onboarding","messages":[{"role":"user","content":{"type":"text","text":GUIDE}}]}),
        ),
        "tools/call" => {
            if !params["name"].is_string() || !params.get("arguments").is_none_or(Value::is_object)
            {
                return Ok((
                    200,
                    error(id, -32602, "Tool name/arguments are invalid", Value::Null),
                ));
            }
            let Some(tool) = tools().into_iter().find(|t| t["name"] == params["name"]) else {
                return Ok((200, error(id, -32602, "Unknown tool", Value::Null)));
            };
            if let Err(message) = validate(
                params.get("arguments").unwrap_or(&json!({})),
                &tool["inputSchema"],
                "arguments",
            ) {
                return Ok((200, error(id, -32602, &message, Value::Null)));
            }
            let result = owner(method, params, id.clone());
            match result {
                Ok(mut value) => {
                    let mut content = value
                        .as_object_mut()
                        .and_then(|v| v.remove("_mcpContent"))
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();
                    content.push(json!({"type":"text","text":value.to_string()}));
                    Ok(json!({"content":content,"structuredContent":value,"isError":false}))
                }
                Err(e) => Ok(json!({"content":[{"type":"text","text":e}],"isError":true})),
            }
        }
        "resources/read" => owner(method, params, id.clone()),
        _ => {
            return Ok((
                if modern { 404 } else { 200 },
                error(id, -32601, "Method not found", Value::Null),
            ));
        }
    };
    match result {
        Ok(mut result) => {
            if modern {
                result["resultType"] = json!("complete");
                result["_meta"]["io.modelcontextprotocol/serverInfo"] = info();
            }
            Ok((200, json!({"jsonrpc":"2.0","id":id,"result":result})))
        }
        Err(e) => Ok((200, error(id, -32602, &e, Value::Null))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn invoke(method: &str, params: Value) -> (u16, Value) {
        dispatch(
            json!({"jsonrpc":"2.0","id":7,"method":method,"params":params}),
            &BTreeMap::new(),
            |_, _, _| Ok(json!({"revision":4})),
        )
        .unwrap()
    }
    #[test]
    fn legacy_handshake_and_discovery_publish_real_tools_resources_and_shared_api() {
        for version in &VERSIONS[1..] {
            let (_, reply) = invoke("initialize", json!({"protocolVersion":version}));
            assert_eq!(*version, reply["result"]["protocolVersion"]);
        }
        let (_, reply) = invoke("tools/list", json!({}));
        assert!(
            reply["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["name"] == "meshcraft_script_api")
        );
        assert!(!tools().iter().any(|t| t["name"] == "k3d_execute_console"));
        assert!(
            resources()
                .iter()
                .any(|r| r["uri"] == "meshcraft://scripting/api")
        );
        assert!(
            crate::script_api::catalog()
                .iter()
                .any(|e| e.name == "rotate" && !e.signature.is_empty() && !e.help.is_empty())
        );
        let (_, result) = invoke("tools/call", json!({"name":"k3d_status","arguments":{}}));
        assert_eq!(4, result["result"]["structuredContent"]["revision"]);
    }
    #[test]
    fn modern_metadata_and_protocol_errors_are_checked_before_owner_dispatch() {
        let mut headers = BTreeMap::from([
            ("mcp-protocol-version".into(), VERSIONS[0].into()),
            ("mcp-method".into(), "server/discover".into()),
        ]);
        let params = json!({"_meta":{"io.modelcontextprotocol/protocolVersion":VERSIONS[0],"io.modelcontextprotocol/clientInfo":{"name":"test","version":"1"},"io.modelcontextprotocol/clientCapabilities":{}}});
        let request = json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":params});
        let (_, reply) = dispatch(request.clone(), &headers, |_, _, _| {
            panic!("discovery cannot touch UI")
        })
        .unwrap();
        assert_eq!("complete", reply["result"]["resultType"]);
        headers.insert("mcp-method".into(), "tools/call".into());
        let (status, _) = dispatch(request, &headers, |_, _, _| {
            panic!("bad metadata cannot touch UI")
        })
        .unwrap();
        assert_eq!(400, status);
        headers.insert("mcp-protocol-version".into(), "1900-01-01".into());
        let (status, reply) = dispatch(
            json!({"jsonrpc":"2.0","id":1,"method":"ping"}),
            &headers,
            |_, _, _| unreachable!(),
        )
        .unwrap();
        assert_eq!(400, status);
        assert_eq!(-32022, reply["error"]["code"]);
        let (_, reply) = invoke(
            "tools/call",
            json!({"name":"k3d_pointer_event","arguments":{"action":"destroy"}}),
        );
        assert_eq!(-32602, reply["error"]["code"]);
    }
    #[test]
    fn notifications_do_not_wait_on_owner_and_tool_failures_are_is_error() {
        let (status, _) = dispatch(
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            &BTreeMap::new(),
            |_, _, _| panic!("initialized cannot wait on owner"),
        )
        .unwrap();
        assert_eq!(202, status);
        let (_, reply) = dispatch(
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"k3d_status"}}),
            &BTreeMap::new(),
            |_, _, _| Err("application closed".into()),
        )
        .unwrap();
        assert_eq!(true, reply["result"]["isError"]);
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;
    #[test]
    fn viewport_png_is_image_content_and_metadata_stays_structured() {
        let (_,reply)=dispatch(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"meshcraft_capture_view"}}),&BTreeMap::new(),|_,_,_|Ok(json!({"_mcpContent":[{"type":"image","data":"png-data","mimeType":"image/png"}],"fresh":true}))).unwrap();
        assert_eq!("image", reply["result"]["content"][0]["type"]);
        assert_eq!(true, reply["result"]["structuredContent"]["fresh"]);
        assert!(
            reply["result"]["structuredContent"]
                .get("_mcpContent")
                .is_none()
        );
        assert!(
            !reply["result"]["content"][1]["text"]
                .as_str()
                .unwrap()
                .contains("png-data")
        );
    }
}
