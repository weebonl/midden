use super::api_paths as paths;

pub(super) fn document() -> serde_json::Value {
    serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Midden API",
            "version": env!("CARGO_PKG_VERSION")
        },
        "paths": {
            (paths::FILES): {
                "post": {
                    "summary": "Upload a file",
                    "security": [{}, {"bearer": []}],
                    "x-required-scope": "files:write",
                    "requestBody": { "required": true, "content": { "multipart/form-data": { "schema": { "$ref": "#/components/schemas/FileUpload" } } } },
                    "responses": { "200": response("ApiUploadResponse"), "400": error_response("Invalid upload"), "403": error_response("Upload forbidden"), "413": error_response("Upload too large"), "429": error_response("Rate limited") }
                }
            },
            (paths::FILE): {
                "delete": { "summary": "Delete a file", "security": [{}, {"bearer": []}], "x-required-scope": "files:delete", "parameters": [path_parameter("id", "Public file id"), header_parameter("x-delete-token", "Anonymous delete token")], "responses": { "200": response("DeletedResponse"), "401": error_response("Authentication required"), "403": error_response("Delete forbidden"), "404": error_response("File not found") } }
            },
            (paths::PASTES): {
                "post": { "summary": "Create a paste", "security": [{}, {"bearer": []}], "x-required-scope": "pastes:write", "requestBody": json_request("PasteRequest"), "responses": { "200": response("PasteCreatedResponse"), "400": error_response("Invalid paste"), "403": error_response("Paste creation forbidden"), "413": error_response("Paste too large"), "429": error_response("Rate limited") } }
            },
            (paths::PASTE): {
                "delete": { "summary": "Delete a paste", "security": [{}, {"bearer": []}], "x-required-scope": "pastes:delete", "parameters": [path_parameter("id", "Public paste id"), header_parameter("x-delete-token", "Anonymous delete token")], "responses": { "200": response("DeletedResponse"), "401": error_response("Authentication required"), "403": error_response("Delete forbidden"), "404": error_response("Paste not found") } }
            },
            (paths::MY_FILES): {
                "get": { "summary": "List authenticated account files", "security": [{"bearer": []}], "x-required-scope": "files:read", "parameters": [query_parameter("q", "Optional file search")], "responses": { "200": response("FileItemsResponse"), "401": error_response("Authentication required"), "429": error_response("Rate limited") } }
            },
            (paths::MY_PASTES): {
                "get": { "summary": "List authenticated account pastes", "security": [{"bearer": []}], "x-required-scope": "pastes:read", "parameters": [query_parameter("q", "Optional paste search")], "responses": { "200": response("PasteItemsResponse"), "401": error_response("Authentication required"), "429": error_response("Rate limited") } }
            },
            (paths::CLAIM): {
                "post": { "summary": "Claim an anonymous item", "security": [{"bearer": []}], "x-required-scope": "items:claim", "parameters": [path_parameter("kind", "file or paste"), path_parameter("id", "Public item id")], "requestBody": json_request("ClaimRequest"), "responses": { "200": response("ClaimedResponse"), "400": error_response("Invalid claim token"), "401": error_response("Authentication required"), "403": error_response("Claim forbidden"), "404": error_response("Item not found") } }
            },
            (paths::REPORTS): {
                "post": { "summary": "Report a file or paste", "security": [{}, {"bearer": []}], "x-required-scope": "reports:write", "requestBody": json_request("ReportRequest"), "responses": { "200": response("ReportedResponse"), "400": error_response("Invalid report"), "403": error_response("Reports disabled"), "429": error_response("Rate limited") } }
            },
            (paths::TOKENS): {
                "get": { "summary": "List account API tokens", "security": [{"bearer": []}], "x-required-scope": "tokens:read", "responses": { "200": response("TokenItemsResponse"), "401": error_response("Authentication required") } },
                "post": { "summary": "Create an account API token", "security": [{"bearer": []}], "x-required-scope": "tokens:write", "requestBody": json_request("TokenCreateRequest"), "responses": { "200": response("TokenCreatedResponse"), "400": error_response("Invalid token request"), "401": error_response("Authentication required"), "403": error_response("Requested scopes exceed caller scopes"), "429": error_response("Rate limited") } }
            },
            (paths::TOKEN): {
                "delete": { "summary": "Revoke an account API token", "security": [{"bearer": []}], "x-required-scope": "tokens:write", "parameters": [path_parameter("id", "Token id")], "responses": { "200": response("RevokedResponse"), "401": error_response("Authentication required"), "404": error_response("Token not found") } }
            },
            (paths::ADMIN_REPORTS): {
                "get": { "summary": "List moderation reports", "security": [{"bearer": []}], "x-required-scope": "admin:reports", "parameters": [query_parameter("state", "Report state"), query_parameter("kind", "Item kind"), query_parameter("reason", "Reason search"), integer_query_parameter("days", "Only recent reports")], "responses": { "200": response("ReportItemsResponse"), "401": error_response("Authentication required"), "403": error_response("Moderator role required") } }
            },
            (paths::ADMIN_REPORT): {
                "patch": { "summary": "Update a moderation report", "security": [{"bearer": []}], "x-required-scope": "admin:reports", "parameters": [path_parameter("id", "Report id")], "requestBody": json_request("ReportActionRequest"), "responses": { "200": response("UpdatedResponse"), "400": error_response("Invalid moderation action"), "401": error_response("Authentication required"), "403": error_response("Moderator role required"), "404": error_response("Report not found") } }
            },
            (paths::ADMIN_ITEM): {
                "patch": { "summary": "Update item moderation state, visibility, notes, or blocked hash", "security": [{"bearer": []}], "x-required-scope": "admin:items", "parameters": [path_parameter("kind", "file or paste"), path_parameter("id", "Public item id")], "requestBody": json_request("AdminItemUpdate"), "responses": { "200": response("UpdatedResponse"), "400": error_response("Invalid item update"), "401": error_response("Authentication required"), "403": error_response("Moderator role required"), "404": error_response("Item not found") } }
            },
            (paths::ADMIN_SEARCH): {
                "get": { "summary": "Search file and paste metadata as a moderator", "security": [{"bearer": []}], "x-required-scope": "admin:search", "parameters": [query_parameter("q", "Search query"), boolean_query_parameter("paste_content", "Search paste content")], "responses": { "200": response("SearchResponse"), "401": error_response("Authentication required"), "403": error_response("Moderator role required") } }
            }
        },
        "components": {
            "securitySchemes": {
                "bearer": { "type": "http", "scheme": "bearer", "bearerFormat": "mdd_* API token" }
            },
            "schemas": {
                "Error": { "type": "object", "required": ["error"], "properties": { "error": { "type": "object", "required": ["status", "code", "message"], "properties": { "status": { "type": "integer" }, "code": { "type": "string" }, "message": { "type": "string" } } } } },
                "FileUpload": { "type": "object", "required": ["file"], "properties": { "file": { "type": "string", "format": "binary" }, "expires": { "type": "string" }, "visibility": { "type": "string", "enum": ["unlisted", "public", "private"] } } },
                "ApiUploadResponse": { "type": "object", "required": ["id", "url", "raw_url"], "properties": { "id": { "type": "string" }, "url": { "type": "string", "format": "uri" }, "raw_url": { "type": "string", "format": "uri" }, "internal_url": { "type": ["string", "null"], "format": "uri" }, "delete_token": { "type": ["string", "null"] } } },
                "FileItem": file_item_schema(),
                "PasteItem": paste_item_schema(),
                "FileItemsResponse": items_schema("FileItem"), "PasteItemsResponse": items_schema("PasteItem"), "TokenItemsResponse": items_schema("TokenSummary"), "ReportItemsResponse": items_schema("Report"),
                "PasteRequest": { "type": "object", "required": ["content"], "properties": { "title": { "type": ["string", "null"] }, "syntax": { "type": ["string", "null"] }, "expires": { "type": ["string", "null"] }, "visibility": { "type": ["string", "null"] }, "content": { "type": "string" } } },
                "PasteCreatedResponse": paste_created_schema(), "DeletedResponse": boolean_schema("deleted"), "ClaimedResponse": boolean_schema("claimed"), "ReportedResponse": boolean_schema("reported"), "RevokedResponse": boolean_schema("revoked"), "UpdatedResponse": boolean_schema("updated"),
                "ClaimRequest": { "type": "object", "required": ["delete_token"], "properties": { "delete_token": { "type": "string" } } },
                "ReportRequest": { "type": "object", "required": ["kind", "id", "reason"], "properties": { "kind": { "type": "string", "enum": ["file", "paste"] }, "id": { "type": "string" }, "reason": { "type": "string" }, "details": { "type": ["string", "null"] } } },
                "ReportActionRequest": { "type": "object", "required": ["action"], "properties": { "action": { "type": "string", "enum": ["resolve", "dismiss", "quarantine", "takedown", "legal_hold"] }, "note": { "type": ["string", "null"] } } },
                "AdminItemUpdate": { "type": "object", "properties": { "state": { "type": ["string", "null"], "enum": ["active", "quarantined", "takedown", "legal_hold", "deleted", null] }, "visibility": { "type": ["string", "null"], "enum": ["unlisted", "public", "private", null] }, "note": { "type": ["string", "null"] }, "block_hash": { "type": ["boolean", "null"] } } },
                "TokenCreateRequest": { "type": "object", "required": ["name", "scopes"], "properties": { "name": { "type": "string" }, "scopes": { "type": "array", "items": { "type": "string" } }, "expires_in_seconds": { "type": ["integer", "null"], "format": "int64" } } },
                "TokenCreatedResponse": token_created_schema(), "TokenSummary": token_summary_schema(), "Report": report_schema(),
                "SearchResponse": { "type": "object", "required": ["files", "pastes"], "properties": { "files": { "type": "array", "items": { "$ref": "#/components/schemas/FileItem" } }, "pastes": { "type": "array", "items": { "$ref": "#/components/schemas/PasteItem" } } } }
            }
        }
    })
}

fn response(schema: &str) -> serde_json::Value {
    serde_json::json!({ "description": "Success", "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{schema}") } } } })
}
fn error_response(description: &str) -> serde_json::Value {
    serde_json::json!({ "description": description, "content": { "application/json": { "schema": { "$ref": "#/components/schemas/Error" } } } })
}
fn json_request(schema: &str) -> serde_json::Value {
    serde_json::json!({ "required": true, "content": { "application/json": { "schema": { "$ref": format!("#/components/schemas/{schema}") } } } })
}
fn path_parameter(name: &str, description: &str) -> serde_json::Value {
    serde_json::json!({ "name": name, "in": "path", "required": true, "description": description, "schema": { "type": "string" } })
}
fn header_parameter(name: &str, description: &str) -> serde_json::Value {
    serde_json::json!({ "name": name, "in": "header", "required": false, "description": description, "schema": { "type": "string" } })
}
fn query_parameter(name: &str, description: &str) -> serde_json::Value {
    serde_json::json!({ "name": name, "in": "query", "required": false, "description": description, "schema": { "type": "string" } })
}
fn integer_query_parameter(name: &str, description: &str) -> serde_json::Value {
    serde_json::json!({ "name": name, "in": "query", "required": false, "description": description, "schema": { "type": "integer", "format": "int64" } })
}
fn boolean_query_parameter(name: &str, description: &str) -> serde_json::Value {
    serde_json::json!({ "name": name, "in": "query", "required": false, "description": description, "schema": { "type": "boolean" } })
}
fn boolean_schema(property: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "required": [property],
        "properties": { (property): { "type": "boolean" } }
    })
}
fn items_schema(item: &str) -> serde_json::Value {
    serde_json::json!({ "type": "object", "required": ["items"], "properties": { "items": { "type": "array", "items": { "$ref": format!("#/components/schemas/{item}") } } } })
}
fn file_item_schema() -> serde_json::Value {
    serde_json::json!({ "type": "object", "required": ["id", "url", "raw_url", "size_bytes", "visibility", "state", "created_at"], "properties": { "id": {"type":"string"}, "url": {"type":"string","format":"uri"}, "raw_url": {"type":"string","format":"uri"}, "internal_url": {"type":["string","null"],"format":"uri"}, "thumbnail_url": {"type":["string","null"],"format":"uri"}, "filename": {"type":["string","null"]}, "content_type": {"type":["string","null"]}, "size_bytes": {"type":"integer","format":"int64"}, "image_width": {"type":["integer","null"],"format":"int64"}, "image_height": {"type":["integer","null"],"format":"int64"}, "visibility": {"type":"string"}, "metadata": {}, "expires_at": {"type":["integer","null"],"format":"int64"}, "state": {"type":"string"}, "created_at": {"type":"integer","format":"int64"} } })
}
fn paste_item_schema() -> serde_json::Value {
    serde_json::json!({ "type": "object", "required": ["id", "url", "raw_url", "size_bytes", "visibility", "state", "created_at"], "properties": { "id": {"type":"string"}, "url": {"type":"string","format":"uri"}, "raw_url": {"type":"string","format":"uri"}, "title": {"type":["string","null"]}, "syntax": {"type":["string","null"]}, "size_bytes": {"type":"integer"}, "visibility": {"type":"string"}, "expires_at": {"type":["integer","null"],"format":"int64"}, "state": {"type":"string"}, "created_at": {"type":"integer","format":"int64"} } })
}
fn paste_created_schema() -> serde_json::Value {
    serde_json::json!({ "type":"object", "required":["id","url","raw_url"], "properties": { "id":{"type":"string"}, "url":{"type":"string","format":"uri"}, "raw_url":{"type":"string","format":"uri"}, "delete_token":{"type":["string","null"]} } })
}
fn token_created_schema() -> serde_json::Value {
    serde_json::json!({ "type":"object", "required":["token"], "properties": { "token":{"type":"string"}, "expires_at":{"type":["integer","null"],"format":"int64"} } })
}
fn token_summary_schema() -> serde_json::Value {
    serde_json::json!({ "type":"object", "required":["id","name","scopes","created_at"], "properties": { "id":{"type":"string"}, "name":{"type":"string"}, "scopes":{"type":"array","items":{"type":"string"}}, "expires_at":{"type":["integer","null"],"format":"int64"}, "last_used_at":{"type":["integer","null"],"format":"int64"}, "revoked_at":{"type":["integer","null"],"format":"int64"}, "created_at":{"type":"integer","format":"int64"} } })
}
fn report_schema() -> serde_json::Value {
    serde_json::json!({ "type":"object", "required":["id","item_kind","item_public_id","reason","details","state","created_at"], "properties": { "id":{"type":"string"}, "item_kind":{"type":"string","enum":["file","paste"]}, "item_public_id":{"type":"string"}, "reporter_user_id":{"type":["string","null"]}, "reason":{"type":"string"}, "details":{"type":"string"}, "state":{"type":"string"}, "created_at":{"type":"integer","format":"int64"} } })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use axum::http::StatusCode;

    use super::{document, paths};

    const ROUTES: &[(&str, &str)] = &[
        (paths::FILES, "post"),
        (paths::FILE, "delete"),
        (paths::PASTES, "post"),
        (paths::PASTE, "delete"),
        (paths::MY_FILES, "get"),
        (paths::MY_PASTES, "get"),
        (paths::CLAIM, "post"),
        (paths::REPORTS, "post"),
        (paths::TOKENS, "get"),
        (paths::TOKENS, "post"),
        (paths::TOKEN, "delete"),
        (paths::ADMIN_REPORTS, "get"),
        (paths::ADMIN_REPORT, "patch"),
        (paths::ADMIN_ITEM, "patch"),
        (paths::ADMIN_SEARCH, "get"),
    ];

    #[test]
    fn contract_covers_every_v1_route_and_operation() {
        let document = document();
        let paths = document["paths"].as_object().unwrap();
        let actual = paths
            .iter()
            .flat_map(|(path, operations)| {
                operations
                    .as_object()
                    .unwrap()
                    .keys()
                    .map(move |method| (path.as_str(), method.as_str()))
            })
            .collect::<BTreeSet<_>>();
        let expected = ROUTES.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(actual, expected);

        for (path, method) in ROUTES {
            let operation = &document["paths"][path][method];
            assert!(operation["security"].is_array(), "{method} {path} security");
            assert!(
                operation["x-required-scope"].is_string(),
                "{method} {path} scope"
            );
            let responses = operation["responses"].as_object().unwrap();
            assert!(responses.keys().any(|status| status.starts_with('2')));
            assert!(responses.keys().any(|status| status.starts_with('4')));

            for parameter in path
                .split(['{', '}'])
                .skip(1)
                .step_by(2)
                .filter(|name| !name.is_empty())
            {
                let declared = operation["parameters"]
                    .as_array()
                    .is_some_and(|parameters| {
                        parameters.iter().any(|candidate| {
                            candidate["name"] == parameter
                                && candidate["in"] == "path"
                                && candidate["required"] == true
                        })
                    });
                assert!(declared, "{method} {path} parameter {parameter}");
            }
        }

        let mut references = Vec::new();
        collect_schema_references(&document, &mut references);
        for reference in references {
            let pointer = reference.strip_prefix('#').unwrap();
            assert!(
                document.pointer(pointer).is_some(),
                "unresolved schema reference {reference}"
            );
        }
    }

    #[test]
    fn error_schema_matches_serialized_api_error() {
        let document = document();
        let schema = &document["components"]["schemas"]["Error"];
        let body = serde_json::to_value(super::super::api::ApiErrorResponse::new(
            StatusCode::UNAUTHORIZED,
        ))
        .unwrap();

        assert_eq!(schema["required"], serde_json::json!(["error"]));
        assert_eq!(
            schema["properties"]["error"]["required"],
            serde_json::json!(["status", "code", "message"])
        );
        assert_eq!(body["error"]["status"], 401);
        assert_eq!(body["error"]["code"], "unauthorized");
        assert_eq!(body["error"]["message"], "Unauthorized");
        assert_eq!(
            body["error"]
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            ["status", "code", "message"]
                .into_iter()
                .collect::<BTreeSet<_>>()
        );
    }

    fn collect_schema_references<'a>(value: &'a serde_json::Value, references: &mut Vec<&'a str>) {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(reference) = object.get("$ref").and_then(serde_json::Value::as_str) {
                    references.push(reference);
                }
                for value in object.values() {
                    collect_schema_references(value, references);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    collect_schema_references(value, references);
                }
            }
            _ => {}
        }
    }
}
