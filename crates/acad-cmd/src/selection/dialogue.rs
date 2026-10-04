//! Shared selection continuation: visible picks/windows collect owners until
//! Return (HATCH windows finish at their second corner); an explicit typed
//! ID/ALL/LAST line replaces the collected set.
use super::{entities_in_window, selectable_count, selection};
use crate::{
    input_state::{EditCommand, InputState},
    parse::{point, point_from},
    report::{list_entities, list_report},
    Editor, Effect,
};
use acad_model::Point;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Mode {
    Collect,
    FirstCorner,
    SecondCorner(Point),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Dialogue {
    target: Box<InputState>,
    ids: Vec<usize>,
    mode: Mode,
}
impl Dialogue {
    fn new(target: InputState) -> Self {
        Self {
            target: Box::new(target),
            ids: Vec::new(),
            mode: Mode::Collect,
        }
    }
    pub(crate) fn prompt(&self) -> &'static str {
        match self.mode {
            Mode::Collect => "Select objects: picks, W, IDs/ALL/LAST; Return to finish",
            Mode::FirstCorner if self.completes_window() => "HATCH: lower left corner",
            Mode::FirstCorner => "Selection window: first corner x,y",
            Mode::SecondCorner(_) if self.completes_window() => "HATCH: upper right corner",
            Mode::SecondCorner(_) => "Selection window: opposite corner x,y",
        }
    }
    fn completes_window(&self) -> bool {
        matches!(self.target.as_ref(), InputState::HatchSelection(..))
    }
    fn add(&mut self, id: usize) {
        if let Err(position) = self.ids.binary_search(&id) {
            self.ids.insert(position, id);
        }
    }
}

impl InputState {
    pub(crate) fn is_selection_target(&self) -> bool {
        matches!(
            self,
            Self::BlockSelection(..)
                | Self::WblockSelection(..)
                | Self::EditSelection(_)
                | Self::DisplacedSelection(..)
                | Self::ArraySelection
                | Self::ChangeSelection
                | Self::FilletSelection
                | Self::BreakSelection
                | Self::AreaSelection
                | Self::HatchSelection(..)
                | Self::ListSelection
        )
    }
}

impl Editor {
    pub(crate) fn selection_window_pending(&self) -> bool {
        matches!(&self.state, InputState::Selection(dialogue) if dialogue.mode != Mode::Collect)
    }
    /// Owners collected by mouse/window input, independent of transport text.
    pub fn collected_selection(&self) -> &[usize] {
        match &self.state {
            InputState::Selection(dialogue) => &dialogue.ids,
            _ => &[],
        }
    }
    pub(crate) fn collecting_selection(&self) -> bool {
        matches!(&self.state, InputState::Selection(dialogue) if dialogue.mode == Mode::Collect)
    }
    /// Collect a visible owner without executing the pending command. Misses
    /// leave the pending selector and accumulated set intact.
    pub fn pick_selection_at(
        &mut self,
        point: Point,
        tolerance: f64,
    ) -> Result<Option<usize>, String> {
        if !self.accepts_mouse_selection() {
            return Err("current prompt does not accept entity selection".into());
        }
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err("pick coordinates must be finite".into());
        }
        if matches!(self.state, InputState::BreakSelection) {
            return self.break_pick(point, tolerance);
        }
        let Some(id) = self.pick_entity_at(point, tolerance) else {
            return Ok(None);
        };
        let mut dialogue = match self.state.clone() {
            InputState::Selection(dialogue) => dialogue,
            target => Dialogue::new(target),
        };
        dialogue.add(id);
        self.state = InputState::Selection(dialogue);
        self.status = format!("Selected entity {id}");
        Ok(Some(id))
    }
    pub(crate) fn single_erase_input(&self, line: &str) -> bool {
        let (erase, collected) = match &self.state {
            InputState::Selection(dialogue) => (
                matches!(
                    dialogue.target.as_ref(),
                    InputState::EditSelection(EditCommand::Erase)
                ) && dialogue.mode == Mode::Collect,
                dialogue.ids.as_slice(),
            ),
            state => (
                matches!(state, InputState::EditSelection(EditCommand::Erase)),
                &[][..],
            ),
        };
        erase
            && if line.is_empty() {
                collected.len() == 1
            } else {
                selection(line, selectable_count(&self.drawing)).is_ok_and(|ids| ids.len() == 1)
            }
    }
    pub(crate) fn route_selection_input(&mut self, line: &str) -> Option<Result<Effect, String>> {
        let window = line.eq_ignore_ascii_case("W") || line.eq_ignore_ascii_case("WINDOW");
        match self.state.clone() {
            InputState::Selection(dialogue) => Some(self.submit_selection_dialogue(dialogue, line)),
            target if target.is_selection_target() && window => {
                let mut dialogue = Dialogue::new(target);
                dialogue.mode = Mode::FirstCorner;
                self.state = InputState::Selection(dialogue);
                Some(Ok(Effect::Continue))
            }
            InputState::ListSelection => Some(if line.is_empty() {
                self.state = InputState::Command;
                Ok(Effect::Continue)
            } else {
                self.selected_list(line)
            }),
            _ => None,
        }
    }
    fn selected_list(&mut self, line: &str) -> Result<Effect, String> {
        let ids = selection(line, selectable_count(&self.drawing))?;
        self.status = list_entities(&self.drawing, &ids);
        let report = list_report(&self.drawing, &ids);
        self.state = InputState::Command;
        Ok(Effect::Report(report))
    }
    fn finish_selection(
        &mut self,
        mut dialogue: Dialogue,
        ids: Vec<usize>,
    ) -> Result<Effect, String> {
        if ids.is_empty() {
            self.state = InputState::Command;
            return Ok(Effect::Continue);
        }
        let input = ids
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",");
        self.state = *dialogue.target.clone();
        // BREAK reads a typed comma pair as a pick point, so collected IDs
        // must reach it as IDs rather than as joined text.
        let result = if matches!(self.state, InputState::BreakSelection) {
            self.break_select_ids(ids.clone())
        } else {
            self.submit(&input)
        };
        if result.is_err() {
            // Preserve a retryable selector after cardinality/geometry rejection.
            // Typed input can replace the failed set, or cancel can discard it.
            dialogue.ids = ids;
            self.state = InputState::Selection(dialogue);
        }
        result
    }
    fn submit_selection_dialogue(
        &mut self,
        mut dialogue: Dialogue,
        line: &str,
    ) -> Result<Effect, String> {
        if dialogue.mode == Mode::Collect
            && matches!(dialogue.target.as_ref(), InputState::FilletSelection)
            && line.eq_ignore_ascii_case("R")
        {
            self.state = InputState::FilletRadius;
            return Ok(Effect::Continue);
        }
        match dialogue.mode {
            Mode::FirstCorner => dialogue.mode = Mode::SecondCorner(point(line)?),
            Mode::SecondCorner(first) => {
                let second = point_from(line, first)?;
                let ids = entities_in_window(&self.drawing, first, second);
                if ids.is_empty() {
                    return Err("selection window contains no visible objects".into());
                }
                for id in ids {
                    dialogue.add(id);
                }
                dialogue.mode = Mode::Collect;
                if dialogue.completes_window() {
                    let ids = dialogue.ids.clone();
                    return self.finish_selection(dialogue, ids);
                }
                self.status = format!("{} objects selected; Return to finish", dialogue.ids.len());
            }
            Mode::Collect => {
                if line.eq_ignore_ascii_case("W") || line.eq_ignore_ascii_case("WINDOW") {
                    dialogue.mode = Mode::FirstCorner;
                } else {
                    let ids = if line.is_empty() {
                        dialogue.ids.clone()
                    } else {
                        selection(line, selectable_count(&self.drawing))?
                    };
                    return self.finish_selection(dialogue, ids);
                }
            }
        }
        self.state = InputState::Selection(dialogue);
        Ok(Effect::Continue)
    }
}
