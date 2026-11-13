use serde::{Serialize, de::DeserializeOwned};
use zuruf_core::*;
fn check<T: Serialize + DeserializeOwned>(name: &str, input: &str) {
    let typed: T = serde_json::from_str(input).unwrap();
    let text = serde_json::to_string_pretty(&typed).unwrap() + "\n";
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"));
    if std::env::var_os("UPDATE_FIXTURES").is_some() {
        std::fs::write(&path, &text).unwrap();
    }
    let fixture = std::fs::read_to_string(path).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&fixture).unwrap(),
        serde_json::from_str::<serde_json::Value>(&text).unwrap()
    );
}
#[test]
fn contract_id() {
    check::<Id>("Id", r##""example""##);
}
#[test]
fn contract_timestamp() {
    check::<Timestamp>("Timestamp", r##""example""##);
}
#[test]
fn contract_project() {
    check::<Project>(
        "Project",
        r##"{"id": "example", "name": "example", "repositoryPath": "example", "baseUrl": "example", "t3ProjectId": "example", "defaultProfileId": "example", "createdAt": "example", "updatedAt": "example"}"##,
    );
}
#[test]
fn contract_workspacestrategy() {
    check::<WorkspaceStrategy>("WorkspaceStrategy", r##""new_worktree""##);
}
#[test]
fn contract_agentprofile() {
    check::<AgentProfile>(
        "AgentProfile",
        r##"{"id": "example", "name": "example", "providerInstanceId": "example", "model": "example", "options": {}, "runtimeMode": "example", "interactionMode": "example", "workspaceStrategy": "new_worktree", "baseRef": "example", "worktreePath": "example", "slot": 1}"##,
    );
}
#[test]
fn contract_t3catalog() {
    check::<T3Catalog>(
        "T3Catalog",
        r##"{"fetchedAt": "example", "providers": [{"instanceId": "example", "displayName": "example", "enabled": true, "constraints": ["example"], "models": [{"id": "example", "label": "example", "options": [{"id": "example", "values": ["example"], "default": "example"}]}], "omittedModels": 1}], "runtimeModes": ["example"], "interactionModes": ["example"]}"##,
    );
}
#[test]
fn contract_t3provider() {
    check::<T3Provider>(
        "T3Provider",
        r##"{"instanceId": "example", "displayName": "example", "enabled": true, "constraints": ["example"], "models": [{"id": "example", "label": "example", "options": [{"id": "example", "values": ["example"], "default": "example"}]}], "omittedModels": 1}"##,
    );
}
#[test]
fn contract_t3model() {
    check::<T3Model>(
        "T3Model",
        r##"{"id": "example", "label": "example", "options": [{"id": "example", "values": ["example"], "default": "example"}]}"##,
    );
}
#[test]
fn contract_t3projectref() {
    check::<T3ProjectRef>(
        "T3ProjectRef",
        r##"{"id": "example", "title": "example", "workspaceRoot": "example"}"##,
    );
}
#[test]
fn contract_t3connection() {
    check::<T3Connection>(
        "T3Connection",
        r##"{"state": "disconnected", "version": "example", "transport": "mcp", "capabilities": {"modelOptions": true, "existingWorktree": true, "idempotentLaunch": true, "interrupt": true, "attachments": true, "push": true}, "error": "example"}"##,
    );
}
#[test]
fn contract_sessionmode() {
    check::<SessionMode>("SessionMode", r##""shared""##);
}
#[test]
fn contract_controlstate() {
    check::<ControlState>(
        "ControlState",
        r##"{"owner": "none", "agentId": "example", "leaseExpiresAt": "example", "settling": true}"##,
    );
}
#[test]
fn contract_browsertab() {
    check::<BrowserTab>(
        "BrowserTab",
        r##"{"id": "example", "sessionId": "example", "url": "example", "title": "example", "active": true, "scriptable": true}"##,
    );
}
#[test]
fn contract_browsersession() {
    check::<BrowserSession>(
        "BrowserSession",
        r##"{"id": "example", "projectId": "example", "name": "example", "mode": "shared", "state": "stopped", "agentAccess": true, "control": {"owner": "none", "agentId": "example", "leaseExpiresAt": "example", "settling": true}, "tabs": [{"id": "example", "sessionId": "example", "url": "example", "title": "example", "active": true, "scriptable": true}], "createdAt": "example"}"##,
    );
}
#[test]
fn contract_review() {
    check::<Review>(
        "Review",
        r##"{"id": "example", "projectId": "example", "name": "example", "createdAt": "example"}"##,
    );
}
#[test]
fn contract_batch() {
    check::<Batch>(
        "Batch",
        r##"{"id": "example", "reviewId": "example", "projectId": "example", "seq": 1, "state": "draft", "title": "example", "note": "example", "profileId": "example", "taskId": "example", "createdAt": "example", "sealedAt": "example"}"##,
    );
}
#[test]
fn contract_annotationtype() {
    check::<AnnotationType>("AnnotationType", r##""viewport""##);
}
#[test]
fn contract_rect() {
    check::<Rect>("Rect", r##"{"x": 1, "y": 1, "width": 1, "height": 1}"##);
}
#[test]
fn contract_asset() {
    check::<Asset>(
        "Asset",
        r##"{"id": "example", "kind": "screenshot", "path": "example", "width": 1, "height": 1, "url": "example"}"##,
    );
}
#[test]
fn contract_locatorcandidate() {
    check::<LocatorCandidate>(
        "LocatorCandidate",
        r##"{"kind": "testid", "value": "example", "playwright": "example", "stable": true, "reason": "example"}"##,
    );
}
#[test]
fn contract_domcapture() {
    check::<DomCapture>(
        "DomCapture",
        r##"{"locators": [{"kind": "testid", "value": "example", "playwright": "example", "stable": true, "reason": "example"}], "tag": "example", "role": "example", "name": "example", "text": "example", "attributes": {}, "boundingBox": {"x": 1, "y": 1, "width": 1, "height": 1}, "framePath": ["example"], "shadowPath": ["example"], "parent": "example", "children": ["example"]}"##,
    );
}
#[test]
fn contract_diagnostic() {
    check::<Diagnostic>(
        "Diagnostic",
        r##"{"kind": "network", "level": "error", "text": "example", "url": "example", "line": 1, "count": 1, "ageMs": 1}"##,
    );
}
#[test]
fn contract_annotation() {
    check::<Annotation>(
        "Annotation",
        r##"{"id": "example", "batchId": "example", "reviewId": "example", "projectId": "example", "sessionId": "example", "tabId": "example", "type": "viewport", "order": 1, "url": "example", "route": "example", "pageTitle": "example", "viewport": {"width": 1, "height": 1, "deviceScaleFactor": 1}, "scroll": {"x": 1, "y": 1}, "cursor": {"x": 1, "y": 1}, "rect": {"x": 1, "y": 1, "width": 1, "height": 1}, "dom": {"locators": [{"kind": "testid", "value": "example", "playwright": "example", "stable": true, "reason": "example"}], "tag": "example", "role": "example", "name": "example", "text": "example", "attributes": {}, "boundingBox": {"x": 1, "y": 1, "width": 1, "height": 1}, "framePath": ["example"], "shadowPath": ["example"], "parent": "example", "children": ["example"]}, "assets": [{"id": "example", "kind": "screenshot", "path": "example", "width": 1, "height": 1, "url": "example"}], "transcript": "example", "transcriptState": "none", "segmentIds": ["example"], "note": "example", "priority": "low", "tags": ["example"], "status": "open", "git": {"commit": "example", "branch": "example"}, "createdAt": "example", "updatedAt": "example"}"##,
    );
}
#[test]
fn contract_transcriptsegment() {
    check::<TranscriptSegment>(
        "TranscriptSegment",
        r##"{"id": "example", "batchId": "example", "annotationId": "example", "startedAt": "example", "endedAt": "example", "state": "recording", "originalText": "example"}"##,
    );
}
#[test]
fn contract_taskstatus() {
    check::<TaskStatus>("TaskStatus", r##""finalizing_transcript""##);
}
#[test]
fn contract_task() {
    check::<Task>(
        "Task",
        r##"{"id": "example", "batchId": "example", "projectId": "example", "title": "example", "status": "finalizing_transcript", "remoteStatus": "example", "profile": {"id": "example", "name": "example", "providerInstanceId": "example", "model": "example", "options": {}, "runtimeMode": "example", "interactionMode": "example", "workspaceStrategy": "new_worktree", "baseRef": "example", "worktreePath": "example", "slot": 1}, "providerDisplayName": "example", "threadId": "example", "threadUrl": "example", "workspacePath": "example", "branch": "example", "lastMessage": "example", "error": "example", "pendingRequest": "example", "workspaceConflict": true, "markedDone": true, "annotationCount": 1, "createdAt": "example", "dispatchedAt": "example", "finishedAt": "example"}"##,
    );
}
#[test]
fn contract_taskevent() {
    check::<TaskEvent>(
        "TaskEvent",
        r##"{"id": "example", "taskId": "example", "at": "example", "kind": "status", "status": "finalizing_transcript", "text": "example"}"##,
    );
}
#[test]
fn contract_speechstate() {
    check::<SpeechState>(
        "SpeechState",
        r##"{"state": "unavailable", "pendingSegments": 1, "lastGapMs": 1, "error": "example"}"##,
    );
}
#[test]
fn contract_actionid() {
    check::<ActionId>("ActionId", r##""review.toggle""##);
}
#[test]
fn contract_modifier() {
    check::<Modifier>("Modifier", r##""ctrl""##);
}
#[test]
fn contract_chord() {
    check::<Chord>("Chord", r##"{"mods": ["ctrl"], "code": "example"}"##);
}
#[test]
fn contract_binding() {
    check::<Binding>(
        "Binding",
        r##"{"action": "review.toggle", "scope": "page", "chord": {"mods": ["ctrl"], "code": "example"}}"##,
    );
}
#[test]
fn contract_bindingissue() {
    check::<BindingIssue>(
        "BindingIssue",
        r##"{"action": "review.toggle", "scope": "page", "kind": "duplicate", "detail": "example"}"##,
    );
}
#[test]
fn contract_settings() {
    check::<Settings>(
        "Settings",
        r##"{"bindings": [{"action": "review.toggle", "scope": "page", "chord": {"mods": ["ctrl"], "code": "example"}}], "capture": {"ringColor": "example", "ringRadius": 1, "keepRawScreenshot": true, "mode": "new_entry"}, "speech": {"enabled": true, "backend": "continuous", "model": "example", "language": "example", "attach": "after", "keepAudio": true, "voxtypePath": "example", "provider": "openai", "endpoint": "example", "whisperCppPath": "example"}, "redaction": {"stripQuery": true, "patterns": ["example"]}, "t3": {"cliPath": "example", "pollSeconds": 1}, "ui": {"theme": "system", "language": "de", "openPanelOnReviewStart": true}, "dataDir": "example"}"##,
    );
}
#[test]
fn contract_appstatus() {
    check::<AppStatus>(
        "AppStatus",
        r##"{"protocolVersion": 1, "daemonVersion": "example", "reviewMode": true, "activeProjectId": "example", "activeReviewId": "example", "activeBatchId": "example", "activeSessionId": "example", "pendingAction": "review.toggle", "speech": {"state": "unavailable", "pendingSegments": 1, "lastGapMs": 1, "error": "example"}, "t3": {"state": "disconnected", "version": "example", "transport": "mcp", "capabilities": {"modelOptions": true, "existingWorktree": true, "idempotentLaunch": true, "interrupt": true, "attachments": true, "push": true}, "error": "example"}}"##,
    );
}
#[test]
fn contract_snapshot() {
    check::<Snapshot>(
        "Snapshot",
        r##"{"status": {"protocolVersion": 1, "daemonVersion": "example", "reviewMode": true, "activeProjectId": "example", "activeReviewId": "example", "activeBatchId": "example", "activeSessionId": "example", "pendingAction": "review.toggle", "speech": {"state": "unavailable", "pendingSegments": 1, "lastGapMs": 1, "error": "example"}, "t3": {"state": "disconnected", "version": "example", "transport": "mcp", "capabilities": {"modelOptions": true, "existingWorktree": true, "idempotentLaunch": true, "interrupt": true, "attachments": true, "push": true}, "error": "example"}}, "projects": [{"id": "example", "name": "example", "repositoryPath": "example", "baseUrl": "example", "t3ProjectId": "example", "defaultProfileId": "example", "createdAt": "example", "updatedAt": "example"}], "profiles": [{"id": "example", "name": "example", "providerInstanceId": "example", "model": "example", "options": {}, "runtimeMode": "example", "interactionMode": "example", "workspaceStrategy": "new_worktree", "baseRef": "example", "worktreePath": "example", "slot": 1}], "sessions": [{"id": "example", "projectId": "example", "name": "example", "mode": "shared", "state": "stopped", "agentAccess": true, "control": {"owner": "none", "agentId": "example", "leaseExpiresAt": "example", "settling": true}, "tabs": [{"id": "example", "sessionId": "example", "url": "example", "title": "example", "active": true, "scriptable": true}], "createdAt": "example"}], "reviews": [{"id": "example", "projectId": "example", "name": "example", "createdAt": "example"}], "batches": [{"id": "example", "reviewId": "example", "projectId": "example", "seq": 1, "state": "draft", "title": "example", "note": "example", "profileId": "example", "taskId": "example", "createdAt": "example", "sealedAt": "example"}], "annotations": [{"id": "example", "batchId": "example", "reviewId": "example", "projectId": "example", "sessionId": "example", "tabId": "example", "type": "viewport", "order": 1, "url": "example", "route": "example", "pageTitle": "example", "viewport": {"width": 1, "height": 1, "deviceScaleFactor": 1}, "scroll": {"x": 1, "y": 1}, "cursor": {"x": 1, "y": 1}, "rect": {"x": 1, "y": 1, "width": 1, "height": 1}, "dom": {"locators": [{"kind": "testid", "value": "example", "playwright": "example", "stable": true, "reason": "example"}], "tag": "example", "role": "example", "name": "example", "text": "example", "attributes": {}, "boundingBox": {"x": 1, "y": 1, "width": 1, "height": 1}, "framePath": ["example"], "shadowPath": ["example"], "parent": "example", "children": ["example"]}, "assets": [{"id": "example", "kind": "screenshot", "path": "example", "width": 1, "height": 1, "url": "example"}], "transcript": "example", "transcriptState": "none", "segmentIds": ["example"], "note": "example", "priority": "low", "tags": ["example"], "status": "open", "git": {"commit": "example", "branch": "example"}, "createdAt": "example", "updatedAt": "example"}], "segments": [{"id": "example", "batchId": "example", "annotationId": "example", "startedAt": "example", "endedAt": "example", "state": "recording", "originalText": "example"}], "tasks": [{"id": "example", "batchId": "example", "projectId": "example", "title": "example", "status": "finalizing_transcript", "remoteStatus": "example", "profile": {"id": "example", "name": "example", "providerInstanceId": "example", "model": "example", "options": {}, "runtimeMode": "example", "interactionMode": "example", "workspaceStrategy": "new_worktree", "baseRef": "example", "worktreePath": "example", "slot": 1}, "providerDisplayName": "example", "threadId": "example", "threadUrl": "example", "workspacePath": "example", "branch": "example", "lastMessage": "example", "error": "example", "pendingRequest": "example", "workspaceConflict": true, "markedDone": true, "annotationCount": 1, "createdAt": "example", "dispatchedAt": "example", "finishedAt": "example"}], "settings": {"bindings": [{"action": "review.toggle", "scope": "page", "chord": {"mods": ["ctrl"], "code": "example"}}], "capture": {"ringColor": "example", "ringRadius": 1, "keepRawScreenshot": true, "mode": "new_entry"}, "speech": {"enabled": true, "backend": "continuous", "model": "example", "language": "example", "attach": "after", "keepAudio": true, "voxtypePath": "example", "provider": "openai", "endpoint": "example", "whisperCppPath": "example"}, "redaction": {"stripQuery": true, "patterns": ["example"]}, "t3": {"cliPath": "example", "pollSeconds": 1}, "ui": {"theme": "system", "language": "de", "openPanelOnReviewStart": true}, "dataDir": "example"}}"##,
    );
}
#[test]
fn contract_notice() {
    check::<Notice>(
        "Notice",
        r##"{"level": "info", "code": "example", "detail": "example", "relatedId": "example"}"##,
    );
}
#[test]
fn contract_apierror() {
    check::<ApiError>(
        "ApiError",
        r##"{"error": {"code": "example", "message": "example", "issues": [{"action": "review.toggle", "scope": "page", "kind": "duplicate", "detail": "example"}]}}"##,
    );
}
#[test]
fn contract_dispatchrequest() {
    check::<DispatchRequest>(
        "DispatchRequest",
        r##"{"profileId": "example", "followUpTaskId": "example"}"##,
    );
}
#[test]
fn contract_dispatchresponse() {
    check::<DispatchResponse>(
        "DispatchResponse",
        r##"{"sealedBatch": {"id": "example", "reviewId": "example", "projectId": "example", "seq": 1, "state": "draft", "title": "example", "note": "example", "profileId": "example", "taskId": "example", "createdAt": "example", "sealedAt": "example"}, "task": {"id": "example", "batchId": "example", "projectId": "example", "title": "example", "status": "finalizing_transcript", "remoteStatus": "example", "profile": {"id": "example", "name": "example", "providerInstanceId": "example", "model": "example", "options": {}, "runtimeMode": "example", "interactionMode": "example", "workspaceStrategy": "new_worktree", "baseRef": "example", "worktreePath": "example", "slot": 1}, "providerDisplayName": "example", "threadId": "example", "threadUrl": "example", "workspacePath": "example", "branch": "example", "lastMessage": "example", "error": "example", "pendingRequest": "example", "workspaceConflict": true, "markedDone": true, "annotationCount": 1, "createdAt": "example", "dispatchedAt": "example", "finishedAt": "example"}, "activeBatch": {"id": "example", "reviewId": "example", "projectId": "example", "seq": 1, "state": "draft", "title": "example", "note": "example", "profileId": "example", "taskId": "example", "createdAt": "example", "sealedAt": "example"}}"##,
    );
}
#[test]
fn contract_followuprequest() {
    check::<FollowUpRequest>(
        "FollowUpRequest",
        r##"{"text": "example", "annotationIds": ["example"]}"##,
    );
}
#[test]
fn contract_resolverequest() {
    check::<ResolveRequest>(
        "ResolveRequest",
        r##"{"resolution": "adopt_thread", "threadId": "example"}"##,
    );
}
#[test]
fn contract_controlrequest() {
    check::<ControlRequest>(
        "ControlRequest",
        r##"{"action": "takeover", "agentId": "example", "leaseSeconds": 1}"##,
    );
}
#[test]
fn contract_serverevent() {
    let snapshot: Snapshot = serde_json::from_str(include_str!("fixtures/Snapshot.json")).unwrap();
    let input = serde_json::to_string(&ServerEvent::Snapshot(snapshot)).unwrap();
    check::<ServerEvent>("ServerEvent", &input);
}
#[test]
fn contract_serverenvelope() {
    let snapshot: Snapshot = serde_json::from_str(include_str!("fixtures/Snapshot.json")).unwrap();
    let input = serde_json::to_string(&ServerEnvelope {
        seq: 1,
        event: ServerEvent::Snapshot(snapshot),
    })
    .unwrap();
    check::<ServerEnvelope>("ServerEnvelope", &input);
}
#[test]
fn contract_speechprovider() {
    check::<SpeechProvider>("SpeechProvider", r##""openai_compatible""##);
}
#[test]
fn contract_speechservice() {
    check::<SpeechService>(
        "SpeechService",
        r##"{"id": "deepgram", "label": "example", "supported": true, "unsupportedReason": "example", "local": false, "needsKey": true, "key": "environment", "keyEnv": "example", "defaultModel": "example", "defaultEndpoint": "example"}"##,
    );
}
#[test]
fn legacy_speech_settings_keep_voxtype() {
    let speech: SettingsSpeech = serde_json::from_str(
        r##"{"enabled": false, "backend": "continuous", "model": null, "language": null, "attach": "after", "keepAudio": false, "voxtypePath": "voxtype"}"##,
    )
    .unwrap();
    assert_eq!(speech.provider, SpeechProvider::Voxtype);
    assert_eq!(speech.endpoint, None);
    assert_eq!(speech.whisper_cpp_path, "whisper-cli");
}
