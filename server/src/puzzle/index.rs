use crate::puzzle::RealmLink;
const SERIALIZATION_LENGTH: u32 = 2;

pub(crate) struct Index<T> {
  items: Vec<T>,
  selected: u32,
}

pub(crate) enum IndexAsset {
  Bool,
  Int,
  Realm,
}

impl crate::puzzle::PuzzleAsset for IndexAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    match self {
      IndexAsset::Bool => Box::new(Index::<bool> { items: Vec::new(), selected: 0 }) as Box<dyn crate::puzzle::PuzzlePiece>,
      IndexAsset::Int => Box::new(Index::<u32> { items: Vec::new(), selected: 0 }) as Box<dyn crate::puzzle::PuzzlePiece>,
      IndexAsset::Realm => Box::new(Index::<RealmLink> { items: Vec::new(), selected: 0 }) as Box<dyn crate::puzzle::PuzzlePiece>,
    }
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    match self {
      IndexAsset::Bool => load::<bool>(input),
      IndexAsset::Int => load::<u32>(input),
      IndexAsset::Realm => load::<RealmLink>(input),
    }
  }
}
fn load<'a, T>(input: &mut crate::puzzle::InputBuffer) -> crate::puzzle::DeserializationResult<'a>
where
  T: 'a,
  T: Clone,
  T: Send + Sync,
  T: crate::puzzle::DecodeSaved,
  T: crate::puzzle::EncodeValue,
  T: Into<crate::puzzle::PieceValue>,
  T: crate::puzzle::ExtractList,
  T: crate::puzzle::ExtractValue,
{
  crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
  let array_len = rmp::decode::read_array_len(input)?;
  let mut array = Vec::with_capacity(array_len as usize);

  for _ in 0..array_len {
    array.push(T::read(input)?);
  }

  Ok(Box::new(Index { items: array, selected: rmp::decode::read_u32(input)? }) as Box<dyn crate::puzzle::PuzzlePiece>)
}

impl<T> crate::puzzle::PuzzlePiece for Index<T>
where
  T: Clone,
  T: Send + Sync,
  T: crate::puzzle::DecodeSaved,
  T: crate::puzzle::EncodeValue,
  T: Into<crate::puzzle::PieceValue>,
  T: crate::puzzle::ExtractList,
  T: crate::puzzle::ExtractValue,
{
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    let changed = match name {
      puzzleverse_core::PuzzleCommand::Insert => match T::extract_list(value) {
        Some(v) => {
          self.items = v.iter().cloned().collect();
          true
        }
        None => false,
      },
      puzzleverse_core::PuzzleCommand::Set => match value {
        crate::puzzle::PieceValue::Num(s) => {
          self.selected = *s;
          true
        }
        _ => false,
      },
      _ => false,
    };

    if changed {
      vec![crate::puzzle::OutputEvent::Event(
        puzzleverse_core::PuzzleEvent::Changed,
        self.items.get(self.selected as usize).map(|v| v.clone().into()).unwrap_or(crate::puzzle::PieceValue::Empty),
      )]
    } else {
      vec![]
    }
  }
  fn interact(self: &mut Self, _: &puzzleverse_core::InteractionType) -> (puzzleverse_core::InteractionResult, crate::puzzle::SimpleOutputEvents) {
    (puzzleverse_core::InteractionResult::Invalid, vec![])
  }

  fn serialize(self: &Self, output: &mut crate::puzzle::OutputBuffer) -> crate::puzzle::SerializationResult {
    rmp::encode::write_array_len(output, SERIALIZATION_LENGTH)?;
    rmp::encode::write_array_len(output, self.items.len() as u32)?;
    for i in &self.items {
      i.write(output)?;
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
    self.items.get(self.selected as usize).map(|r| state.apply(&r.clone().into())).flatten()
  }
  fn walk(self: &mut Self, _: &crate::PlayerKey, _: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
}
