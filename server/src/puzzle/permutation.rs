use rand::seq::SliceRandom;
const SERIALIZATION_LENGTH: u32 = 2;
pub(crate) struct Permutation {
  pbox: Vec<u32>,
  selected: u32,
}

pub(crate) struct PermutationAsset {
  length: u8,
}

impl crate::puzzle::PuzzleAsset for PermutationAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    let mut pbox: Vec<u32> = (0..self.length as u32).collect();
    pbox.shuffle(&mut rand::thread_rng());
    Box::new(Permutation { pbox, selected: 0 }) as Box<dyn crate::puzzle::PuzzlePiece>
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
    let array_len = rmp::decode::read_array_len(input)?;
    let mut array = Vec::with_capacity(array_len as usize);

    for _ in 0..array_len {
      array.push(rmp::decode::read_u32(input)?);
    }

    Ok(Box::new(Permutation { pbox: array, selected: rmp::decode::read_u32(input)? }) as Box<dyn crate::puzzle::PuzzlePiece>)
  }
}

impl crate::puzzle::PuzzlePiece for Permutation {
  fn accept<'a>(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    let (shuffled, new_selected) = match (name, value) {
      (puzzleverse_core::PuzzleCommand::Set, crate::puzzle::PieceValue::Num(v)) => {
        (false, std::cmp::max(0, std::cmp::min(*v, self.pbox.len() as u32)))
      }
      (puzzleverse_core::PuzzleCommand::Set, crate::puzzle::PieceValue::Empty) => {
        self.pbox.shuffle(&mut rand::thread_rng());
        (true, self.selected)
      }
      _ => (false, self.selected),
    };

    if shuffled {
      self.selected = new_selected;
      vec![
        crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Changed, crate::puzzle::PieceValue::NumList(self.pbox.clone())),
        crate::puzzle::OutputEvent::Event(puzzleverse_core::PuzzleEvent::Selected, crate::puzzle::PieceValue::Num(self.pbox[self.selected as usize])),
      ]
    } else if new_selected == self.selected {
      vec![]
    } else {
      self.selected = new_selected;
      vec![crate::puzzle::OutputEvent::Event(
        puzzleverse_core::PuzzleEvent::Selected,
        crate::puzzle::PieceValue::Num(self.pbox[self.selected as usize]),
      )]
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
    rmp::encode::write_array_len(output, self.pbox.len() as u32)?;
    for &i in &self.pbox {
      rmp::encode::write_u32(output, i)?;
    }
    rmp::encode::write_u32(output, self.selected)
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
    self.pbox.get(self.selected as usize).map(|&r| state.apply(&r.into())).flatten()
  }
  fn walk(self: &mut Self, _: &crate::PlayerKey, _: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
}
