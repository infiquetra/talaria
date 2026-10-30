//! Wire the compiler's cards into board state. The two halves are not connected.

use talaria_core::kanban::KanbanBoardState;
use talaria_workflow::kanban_compile::PlannedCard;

pub fn cards_to_board(cards: &[PlannedCard]) -> Result<KanbanBoardState, String> {
    let _ = cards;
    Err("board view is not connected to compile output".into())
}
