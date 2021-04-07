use crate::puzzle::{EncodeValue, ExtractValue};

const SERIALIZATION_LENGTH: u32 = 2;

struct Logic {
  left: bool,
  right: bool,
  operation: LogicOperation,
}

#[derive(Copy, Clone)]
enum LogicOperation {
  And,
  Or,
  ExclusiveOr,
  NAnd,
  NOr,
}

struct LogicAsset(LogicOperation);

impl LogicOperation {
  fn perform(&self, left: bool, right: bool) -> bool {
    match self {
      LogicOperation::And => left && right,
      LogicOperation::Or => left || right,
      LogicOperation::ExclusiveOr => left ^ right,
      LogicOperation::NAnd => !(left && right),
      LogicOperation::NOr => !(left || right),
    }
  }
}

impl crate::puzzle::PuzzleAsset for LogicAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    Box::new(Logic { left: false, right: false, operation: self.0 }) as Box<dyn crate::puzzle::PuzzlePiece>
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
    Ok(Box::new(Logic { left: rmp::decode::read_bool(input)?, right: rmp::decode::read_bool(input)?, operation: self.0 })
      as Box<dyn crate::puzzle::PuzzlePiece>)
  }
}

impl crate::puzzle::PuzzlePiece for Logic {
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    let old_state = self.operation.perform(self.left, self.right);

    let update = bool::extract_value(value)
      .map(|v| {
        if name == &puzzleverse_core::PuzzleCommand::SetLeft {
          self.left = v;
          true
        } else if name == &puzzleverse_core::PuzzleCommand::SetRight {
          self.right = v;
          true
        } else {
          false
        }
      })
      .unwrap_or_else(|| false);

    let new_state = self.operation.perform(self.left, self.right);
    if update && old_state != new_state {
      vec![crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Changed, crate::puzzle::PieceValue::Bool(new_state))]
    } else {
      vec![]
    }
  }
  fn interact(self: &mut Self, _: &puzzleverse_core::InteractionType) -> (puzzleverse_core::InteractionResult, crate::puzzle::SimpleOutputEvents) {
    (puzzleverse_core::InteractionResult::Invalid, vec![])
  }

  fn serialize(self: &Self, output: &mut crate::puzzle::OutputBuffer) -> crate::puzzle::SerializationResult {
    rmp::encode::write_array_len(output, SERIALIZATION_LENGTH)?;
    self.left.write(output)?;
    self.right.write(output)
  }
  fn tick(self: &mut Self, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
  fn next(self: &Self) -> Option<chrono::DateTime<chrono::Utc>> {
    None
  }
  fn reset(&self) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
  fn update_check<'a, 's>(self: &'s Self, state: &'a crate::puzzle::ConsequenceValueMatcher) -> Option<crate::puzzle::PuzzleConsequence<'a>> {
    state.apply(&(self.left == self.right).into())
  }
  fn walk(self: &mut Self, _: &crate::PlayerKey, _: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
}
