//! Pure board state. Loading a workflow does not yet offer moves.

mod state;

pub use state::{
    BoardCard, CardComment, FrameCard, FrameColumn, KanbanBoardState, KanbanFrame, MoveTable,
};
