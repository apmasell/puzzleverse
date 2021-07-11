use crate::abs_difference;
use crate::puzzle::{EncodeValue, ExtractValue};

const SERIALIZATION_LENGTH: u32 = 2;

struct Arithmetic {
  left: u32,
  right: u32,
  operation: ArithmeticOperation,
}

#[derive(Copy, Clone)]
enum ArithmeticOperation {
  Add,
  Subtract,
  Multiply,
  Divide,
  Modulo,
  AbsoluteDifference,
}

struct ArithmeticAsset(ArithmeticOperation);

impl ArithmeticOperation {
  fn perform(&self, left: u32, right: u32) -> u32 {
    match self {
      ArithmeticOperation::Add => left + right,
      ArithmeticOperation::Subtract => left - right,
      ArithmeticOperation::Multiply => left * right,
      ArithmeticOperation::Divide => {
        if right == 0 {
          left
        } else {
          left / right
        }
      }
      ArithmeticOperation::Modulo => {
        if right == 0 {
          left
        } else {
          left % right
        }
      }
      ArithmeticOperation::AbsoluteDifference => abs_difference(left, right),
    }
  }
}

impl crate::puzzle::PuzzleAsset for ArithmeticAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    Box::new(Arithmetic { left: 0, right: 0, operation: self.0 }) as Box<dyn crate::puzzle::PuzzlePiece>
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
    Ok(Box::new(Arithmetic { left: rmp::decode::read_u32(input)?, right: rmp::decode::read_u32(input)?, operation: self.0 })
      as Box<dyn crate::puzzle::PuzzlePiece>)
  }
}

impl crate::puzzle::PuzzlePiece for Arithmetic {
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    let old_state = self.operation.perform(self.left, self.right);

    let update = u32::extract_value(value)
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
      vec![crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Changed, crate::puzzle::PieceValue::Num(new_state))]
    } else {
      vec![]
    }
  }
  fn interact(
    self: &mut Self,
    _: &puzzleverse_core::InteractionType,
    _: &str,
  ) -> (puzzleverse_core::InteractionResult, crate::puzzle::SimpleOutputEvents) {
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
