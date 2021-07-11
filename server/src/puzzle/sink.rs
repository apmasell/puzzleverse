const SERIALIZATION_LENGTH: u32 = 1;
struct Sink<T> {
  value: T,
}

enum SinkAsset {
  Bool,
  Int,
}

impl crate::puzzle::PuzzleAsset for SinkAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    match self {
      SinkAsset::Bool => std::boxed::Box::new(Sink::<bool> { value: false }) as Box<dyn crate::puzzle::PuzzlePiece>,
      SinkAsset::Int => std::boxed::Box::new(Sink::<u32> { value: 0 }) as Box<dyn crate::puzzle::PuzzlePiece>,
    }
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    match self {
      SinkAsset::Bool => load::<bool, crate::puzzle::InputBuffer>(input, &|i| rmp::decode::read_bool(i)),
      SinkAsset::Int => load::<u32, crate::puzzle::InputBuffer>(input, &|i| rmp::decode::read_u32(i)),
    }
  }
}

fn load<'a, T, R>(
  input: &mut crate::puzzle::InputBuffer,
  read_value: &'static dyn Fn(&mut crate::puzzle::InputBuffer) -> Result<T, rmp::decode::ValueReadError>,
) -> crate::puzzle::DeserializationResult<'a>
where
  T: Send + Sync,
  T: Clone,
  T: Into<crate::puzzle::PieceValue>,
  T: crate::puzzle::EncodeValue,
  T: crate::puzzle::ExtractValue,
{
  crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
  Ok(Box::new(Sink::<T> { value: read_value(input)? }) as Box<dyn crate::puzzle::PuzzlePiece>)
}

impl<T> crate::puzzle::PuzzlePiece for Sink<T>
where
  T: Send + Sync,
  T: Clone,
  T: Into<crate::puzzle::PieceValue>,
  T: crate::puzzle::EncodeValue,
  T: crate::puzzle::ExtractValue,
{
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    if name == &puzzleverse_core::PuzzleCommand::Set {
      match T::extract_value(value) {
        Some(v) => self.value = v,
        None => (),
      }
    }
    vec![]
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
    self.value.write(output)
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
    state.apply(&self.value.clone().into())
  }
  fn walk(self: &mut Self, _: &crate::PlayerKey, _: crate::realm::navigation::PlayerNavigationEvent) -> crate::puzzle::SimpleOutputEvents {
    vec![]
  }
}
