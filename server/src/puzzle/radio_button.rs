use crate::puzzle::PuzzlePiece;
const SERIALIZATION_LENGTH: u32 = 2;

pub(crate) struct RadioButton {
  state: u32,
  enabled: bool,
  max: u32,
}

pub(crate) struct RadioButtonAsset {
  initial: u32,
  enabled: bool,
  max: u32,
}

impl crate::puzzle::PuzzleAsset for RadioButtonAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    std::boxed::Box::new(RadioButton { state: self.initial, enabled: self.enabled, max: self.max })
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
    Ok(Box::new(RadioButton { state: rmp::decode::read_u32(input)?, enabled: rmp::decode::read_bool(input)?, max: self.max }) as Box<dyn PuzzlePiece>)
  }
}

impl PuzzlePiece for RadioButton {
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    let (new_enabled, new_state) = match (name, value) {
      (puzzleverse_core::PuzzleCommand::Up, crate::puzzle::PieceValue::Empty) => (self.enabled, std::cmp::min(self.max, self.state + 1)),
      (puzzleverse_core::PuzzleCommand::Up, crate::puzzle::PieceValue::Num(delta)) => (self.enabled, std::cmp::min(self.max, self.state + *delta)),
      (puzzleverse_core::PuzzleCommand::Down, crate::puzzle::PieceValue::Empty) => (self.enabled, if self.state == 0 { 0 } else { self.state - 1 }),
      (puzzleverse_core::PuzzleCommand::Down, crate::puzzle::PieceValue::Num(delta)) => {
        (self.enabled, if self.state < *delta { 0 } else { self.state - *delta })
      }
      (puzzleverse_core::PuzzleCommand::Enable, crate::puzzle::PieceValue::Empty) => (true, self.state),
      (puzzleverse_core::PuzzleCommand::Disable, crate::puzzle::PieceValue::Empty) => (false, self.state),
      (puzzleverse_core::PuzzleCommand::Set, crate::puzzle::PieceValue::Num(v)) => (self.enabled, *v),
      (puzzleverse_core::PuzzleCommand::Enable, crate::puzzle::PieceValue::Bool(v)) => (*v, self.state),
      _ => (self.enabled, self.state),
    };
    let mut results = Vec::new();

    if new_enabled != self.enabled {
      self.enabled = new_enabled;
      results.push(crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Sensitive, crate::puzzle::PieceValue::Bool(self.enabled)));
    }
    if new_state != self.state {
      self.state = new_state;
      results.push(crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Changed, crate::puzzle::PieceValue::Num(self.state)));
    }
    results
  }
  fn interact(
    self: &mut Self,
    interaction: &puzzleverse_core::InteractionType,
  ) -> (puzzleverse_core::InteractionResult, crate::puzzle::SimpleOutputEvents) {
    match interaction {
      puzzleverse_core::InteractionType::Choose(value) => {
        if self.enabled && *value <= self.max {
          self.state = !self.state;
          (puzzleverse_core::InteractionResult::Accepted, vec![(puzzleverse_core::PuzzleEvent::Changed, crate::puzzle::PieceValue::Num(self.state))])
        } else {
          (puzzleverse_core::InteractionResult::Failed, vec![])
        }
      }
      _ => (puzzleverse_core::InteractionResult::Invalid, vec![]),
    }
  }

  fn serialize(self: &Self, output: &mut crate::puzzle::OutputBuffer) -> crate::puzzle::SerializationResult {
    rmp::encode::write_array_len(output, SERIALIZATION_LENGTH)?;
    rmp::encode::write_u32(output, self.state)?;
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
  fn update_check<'a, 's>(self: &'s Self, state: &'a crate::puzzle::ConsequenceValueMatcher) -> Option<crate::puzzle::PuzzleConsequence<'a>> {
    state.apply(&self.state.into())
  }
  fn walk(self: &mut Self, _: &crate::PlayerKey, _: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
}
