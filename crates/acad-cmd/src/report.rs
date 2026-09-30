//! Text reports for HELP, LIST, and DBLIST.

use crate::entity_ops::bare;
use crate::HATCH_PATTERNS;
use acad_model::{Drawing, Entity, Item};

const COMMAND_LIST: &str = "\
Command List

ARC         DELAY       HELP        ORTHO       SKETCH
AREA        DIM         ID          PAN         SNAP
ARRAY       DIST        INSERT      PLOT        SOLID
AXIS        END         LAYER       POINT       STATUS
BASE        ENDREP      LIMITS      QPLOT       TABLET
BLOCK       ERASE       LINE        QUIT        TEXT
BREAK       FILES       LIST        REDRAW      TRACE
CHANGE      FILL        LOAD        REGEN       UNITS
CIRCLE      FILLET      MENU        REPEAT      WBLOCK
COPY        GRID        MOVE        RESUME      ZOOM
DBLIST      HATCH       OOPS        SHAPE       ?

Point Entry: Absolute: x,y; Relative: @dx,dy; Distance, angle: @d<a
Object selection: L = Last object; W = Within window
Command repeat: press space or RETURN.\n";

pub(crate) const LINE_HELP: &str = "\
The  LINE  command allows you to draw straight lines.

Format:     LINE  From point:  <point>
            To point:  <point>
            To point:  <point>
            To point:  RETURN to end line sequence

You can continue the previous line or arc by responding to the
\"From point:\" prompt with a space or RETURN.  If you are drawing
a sequence of lines which will become a closed polygon, you may
reply to the \"to point\" prompt with \"C\" to draw the last segment
(close the polygon).

Lines may be constrained to horizontal or vertical by the ORTHO command.

Reference:  Section 3.1 of User Guide.\n";

pub(crate) fn help_report(input: &str) -> Result<String, String> {
    let command = input.trim().to_ascii_uppercase();
    if command.is_empty() {
        return Ok(COMMAND_LIST.into());
    }
    if command == "LINE" {
        return Ok(LINE_HELP.into());
    }
    Err(format!("help text for {command} has not been recovered"))
}

pub(crate) fn hatch_pattern_report() -> String {
    HATCH_PATTERNS
        .iter()
        .map(|(name, description)| format!("{name:<15} - {description}\n"))
        .collect()
}

pub(crate) fn list_entities(drawing: &Drawing) -> String {
    let mut lines = Vec::new();
    for (index, entity) in drawing
        .entities()
        .filter(|entity| !matches!(bare(entity), Entity::Load { .. }))
        .enumerate()
    {
        let kind = match bare(entity) {
            Entity::Repeat(_) => "REPEAT",
            Entity::Line { .. } => "LINE",
            Entity::Circle { .. } => "CIRCLE",
            Entity::Arc { .. } => "ARC",
            Entity::Text { .. } => "TEXT",
            Entity::Insert { .. } => "INSERT",
            Entity::Point { .. } => "POINT",
            Entity::Trace { .. } => "TRACE",
            Entity::Solid { .. } => "SOLID",
            Entity::Shape { .. } => "SHAPE",
            Entity::Load { .. } | Entity::OnLayer { .. } => unreachable!(),
        };
        lines.push(format!("{} {kind}", index + 1));
    }
    if lines.is_empty() {
        "No entities".into()
    } else {
        lines.join(", ")
    }
}

pub(crate) fn database_listing(drawing: &Drawing) -> String {
    fn append_entity(lines: &mut Vec<String>, count: &mut usize, entity: &Entity) {
        *count += 1;
        lines.push(format!("Entity {}: {:#?}", count, bare(entity)));
    }

    let mut lines = vec!["Drawing database".to_owned()];
    let mut count = 0;
    for item in &drawing.items {
        match item {
            Item::Entity(entity) => append_entity(&mut lines, &mut count, entity),
            Item::Erased(_) => {}
            Item::Block(block) => {
                lines.push(format!("Block {} base {:?}", block.name, block.base));
                for entity in &block.entities {
                    append_entity(&mut lines, &mut count, entity);
                }
            }
            Item::Repeat(repeat) => {
                lines.push(format!(
                    "Repeat {} columns × {} rows",
                    repeat.columns, repeat.rows
                ));
                for entity in &repeat.entities {
                    append_entity(&mut lines, &mut count, entity);
                }
            }
        }
    }
    if count == 0 {
        lines.push("No entities".into());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Editor, Effect};

    #[test]
    fn question_mark_and_help_share_the_observed_command_query_flow() {
        let mut editor = Editor::default();
        let source = editor.drawing().clone();

        editor.submit("?").unwrap();
        assert_eq!(editor.prompt(), "Command name (RETURN for list)");
        let Effect::Report(list) = editor.submit("").unwrap() else {
            panic!("blank ? query should display the command list");
        };
        assert!(list.contains("Command List"));
        assert!(list.contains("LINE"));
        assert!(list.contains("WBLOCK"));
        assert_eq!(editor.prompt(), "Command");

        editor.submit("HELP").unwrap();
        assert_eq!(editor.prompt(), "Command name (RETURN for list)");
        let Effect::Report(line) = editor.submit("LINE").unwrap() else {
            panic!("HELP LINE should display command help");
        };
        assert_eq!(line, LINE_HELP);
        assert_eq!(editor.status(), "Help for LINE");
        assert_eq!(editor.drawing(), &source);

        editor.submit("HELP").unwrap();
        assert!(editor.submit("CIRCLE").is_err());
        assert_eq!(editor.prompt(), "Command name (RETURN for list)");
    }
}
