use crate::puzzle::PuzzlePiece;
const SERIALIZATION_LENGTH: u32 = 1;

pub(crate) struct Button {
  enabled: bool,
}

pub(crate) struct ButtonAsset {
  enabled: bool,
}

impl crate::puzzle::PuzzleAsset for ButtonAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    std::boxed::Box::new(Button { enabled: self.enabled })
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
    Ok(Box::new(Button { enabled: rmp::decode::read_bool(input)? }) as Box<dyn PuzzlePiece>)
  }
}

impl PuzzlePiece for Button {
  fn accept<'a>(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    let new_enabled = match (name, value) {
      (puzzleverse_core::PuzzleCommand::Enable, crate::puzzle::PieceValue::Empty) => true,
      (puzzleverse_core::PuzzleCommand::Disable, crate::puzzle::PieceValue::Empty) => false,
      (puzzleverse_core::PuzzleCommand::Enable, crate::puzzle::PieceValue::Bool(v)) => *v,
      _ => self.enabled,
    };
    if new_enabled == self.enabled {
      vec![]
    } else {
      self.enabled = new_enabled;
      vec![crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Sensitive, crate::puzzle::PieceValue::Bool(self.enabled))]
    }
  }
  fn interact(
    self: &mut Self,
    interaction: &puzzleverse_core::InteractionType,
    _: &str,
  ) -> (puzzleverse_core::InteractionResult, crate::puzzle::SimpleOutputEvents) {
    match interaction {
      puzzleverse_core::InteractionType::Click => {
        if self.enabled {
          (puzzleverse_core::InteractionResult::Accepted, vec![(puzzleverse_core::PuzzleEvent::Changed, crate::puzzle::PieceValue::Empty)])
        } else {
          (puzzleverse_core::InteractionResult::Failed, vec![])
        }
      }
      _ => (puzzleverse_core::InteractionResult::Invalid, vec![]),
    }
  }

  fn serialize(self: &Self, output: &mut crate::puzzle::OutputBuffer) -> crate::puzzle::SerializationResult {
    rmp::encode::write_array_len(output, SERIALIZATION_LENGTH)?;
    rmp::encode::write_bool(output, self.enabled).map_err(rmp::encode::ValueWriteError::InvalidDataWrite)
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
  fn update_check<'a, 's>(self: &'s Self, _: &'a crate::puzzle::ConsequenceValueMatcher) -> Option<crate::puzzle::PuzzleConsequence<'a>> {
    None
  }
  fn walk(self: &mut Self, _: &crate::PlayerKey, _: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
}
