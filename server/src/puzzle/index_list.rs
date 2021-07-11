use crate::puzzle::RealmLink;
const SERIALIZATION_LENGTH: u32 = 2;

pub(crate) struct IndexList<T> {
  items: Vec<T>,
  selected: Vec<u32>,
}

pub(crate) enum IndexListAsset {
  Bool,
  Int,
  Realm,
}

impl crate::puzzle::PuzzleAsset for IndexListAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    match self {
      IndexListAsset::Bool => Box::new(IndexList::<bool> { items: Vec::new(), selected: vec![] }) as Box<dyn crate::puzzle::PuzzlePiece>,
      IndexListAsset::Int => Box::new(IndexList::<u32> { items: Vec::new(), selected: vec![] }) as Box<dyn crate::puzzle::PuzzlePiece>,
      IndexListAsset::Realm => Box::new(IndexList::<RealmLink> { items: Vec::new(), selected: vec![] }) as Box<dyn crate::puzzle::PuzzlePiece>,
    }
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    match self {
      IndexListAsset::Bool => load::<bool>(input),
      IndexListAsset::Int => load::<u32>(input),
      IndexListAsset::Realm => load::<RealmLink>(input),
    }
  }
}
fn load<'a, T>(input: &mut crate::puzzle::InputBuffer) -> crate::puzzle::DeserializationResult<'a>
where
  T: 'a,
  T: Send + Sync,
  T: Clone,
  T: crate::puzzle::DecodeSaved,
  T: crate::puzzle::EncodeValue,
  Vec<T>: Into<crate::puzzle::PieceValue>,
  T: crate::puzzle::ExtractValue,
  T: crate::puzzle::ExtractList,
{
  crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
  let array_len = rmp::decode::read_array_len(input)?;
  let mut array = Vec::with_capacity(array_len as usize);

  for _ in 0..array_len {
    array.push(T::read(input)?);
  }

  let selected_len = rmp::decode::read_array_len(input)?;
  let mut selected = Vec::with_capacity(selected_len as usize);
  for _ in 0..array_len {
    selected.push(rmp::decode::read_u32(input)?);
  }

  Ok(Box::new(IndexList { items: array, selected }) as Box<dyn crate::puzzle::PuzzlePiece>)
}

impl<T> crate::puzzle::PuzzlePiece for IndexList<T>
where
  T: Clone,
  T: Send + Sync,
  T: crate::puzzle::DecodeSaved,
  T: crate::puzzle::EncodeValue,
  Vec<T>: Into<crate::puzzle::PieceValue>,
  T: crate::puzzle::ExtractList,
{
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    let changed = match name {
      puzzleverse_core::PuzzleCommand::Insert => match T::extract_list(value) {
        Some(v) => {
          self.items = v.to_vec();
          true
        }
        None => false,
      },
      puzzleverse_core::PuzzleCommand::Set => match value {
        crate::puzzle::PieceValue::NumList(s) => {
          self.selected = s.to_vec();
          true
        }
        _ => false,
      },
      _ => false,
    };

    if changed {
      vec![crate::puzzle::OutputEvent::Event(
        puzzleverse_core::PuzzleEvent::Changed,
        if self.items.is_empty() {
          crate::puzzle::PieceValue::Empty
        } else {
          self.selected.iter().flat_map(|&i| self.items.get(i as usize % self.items.len())).cloned().collect::<Vec<T>>().into()
        },
      )]
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
    rmp::encode::write_array_len(output, self.items.len() as u32)?;
    for i in &self.items {
      i.write(output)?;
    }
    rmp::encode::write_array_len(output, self.selected.len() as u32)?;
    for &s in &self.selected {
      rmp::encode::write_u32(output, s)?;
    }
    Ok(())
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
