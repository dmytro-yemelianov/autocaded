use serde_json::{json, Value};
pub(super) const NAMES: [&str; 17] = [
    "acad_new",
    "acad_open",
    "acad_command",
    "acad_point",
    "acad_click",
    "acad_motion",
    "acad_state",
    "acad_drawing",
    "acad_save",
    "acad_cancel",
    "acad_report",
    "acad_frame",
    "acad_quit",
    "acad_script",
    "acad_script_status",
    "acad_script_tick",
    "acad_script_stop",
];
pub(super) fn list() -> Value {
    let string = json!({"type":"string"});
    let number = json!({"type":"number"});
    let dim = json!({"type":"integer","minimum":1,"maximum":4096});
    let definitions = [
        ("Create a fresh drawing in the current session.",json!({}),json!([])),
        ("Open DWG/DXF in the current session. Failed opens preserve the current drawing.",json!({"path":string,"directories":{"type":"array","items":string}}),json!(["path"])),
        ("Submit one Return input, including command names, prompt answers, or an empty string. Inspect prompt after every call.",json!({"input":string}),json!(["input"])),
        ("Place a world-coordinate point at the current prompt using SNAP/ORTHO.",json!({"x":number,"y":number}),json!(["x","y"])),
        ("Click a physical client pixel: includes screen-menu, point and selection routes. Dimensions default to the attached window.",json!({"x":number,"y":number,"width":dim,"height":dim}),json!(["x","y"])),
        ("Move the pointer to a physical client pixel without clicking. SKETCH samples it (pen down records at the record increment); other prompts only move the crosshair. Dimensions default to the attached window.",json!({"x":number,"y":number,"width":dim,"height":dim}),json!(["x","y"])),
        ("Read prompt, status, input, document path/format/dirty state, entity/block counts, view and drawing settings.",json!({}),json!([])),
        ("Return current geometry as historical DXF text, without writing a file.",json!({}),json!([])),
        ("Save the current drawing; .dxf uses DXF, other extensions use DWG.",json!({"path":string}),json!(["path"])),
        ("Cancel the current prompt, retaining completed geometry.",json!({}),json!([])),
        ("Navigate the shared report viewer without changing drawing data. Actions: up/down, page_up/page_down, home/end, close/open. Optional dimensions match the frame being viewed.",json!({"action":{"type":"string","enum":["up","down","page_up","page_down","home","end","close","open"]},"width":dim,"height":dim}),json!(["action"])),
        ("Capture the shared GUI renderer including menu, prompt, selection and cursor. PNG returns an MCP image; rgba returns base64 RGBA8 pixels (top-down, stride width*4). Poll for successive frames.",json!({"width":dim,"height":dim,"format":{"type":"string","enum":["png","rgba"],"default":"png"}}),json!([])),
        ("Request QUIT confirmation. Reply Y/YES using acad_command to discard and exit; discard=true explicitly exits immediately without saving. END via acad_command saves before exit.",json!({"discard":{"type":"boolean","default":false}}),json!([])),
        ("Start a command script file (.SCR added when no extension; document directory, then process directory). Runs at most 64 due items now; DELAY pauses without blocking. Then call acad_script_tick again while script state is running, or after delay_remaining_ms while delaying.",json!({"path":string}),json!(["path"])),
        ("Read script state: idle/running/delaying/interrupted, next line and byte offset, delay remaining, interrupt cause/line/message.",json!({}),json!([])),
        ("Run script items that are due; at most 64 per call, never waits. Repeat while script state is running (long scripts) and after delay_remaining_ms while delaying.",json!({}),json!([])),
        ("Discard the current script, including an interrupted one awaiting RESUME.",json!({}),json!([])),
    ];
    json!({"tools":NAMES.iter().zip(definitions).map(|(name,(description,properties,required))|json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":matches!(*name,"acad_state"|"acad_drawing"|"acad_frame"|"acad_script_status"),"openWorldHint":false}})).collect::<Vec<_>>()})
}
