use crate::puzzle::EncodeValue;
const SERIALIZATION_LENGTH: u32 = 2;

struct Comparator<T>
where
  T: Copy,
  T: PartialOrd,
  T: EncodeValue,
  T: Sync,
  T: crate::puzzle::ExtractValue,
{
  left: T,
  right: T,
  operation: ComparatorOperation,
}

enum ComparatorType {
  Bool,
  Int,
}

#[derive(Copy, Clone)]
enum ComparatorOperation {
  Equals,
  NotEquals,
  LessThan,
  LessThanOrEqualTo,
  GreaterThan,
  GreaterThanOrEqualTo,
}

struct ComparatorAsset {
  operation: ComparatorOperation,
  value_type: ComparatorType,
}

impl ComparatorOperation {
  fn compare<T: PartialOrd>(&self, left: T, right: T) -> bool {
    match self {
      ComparatorOperation::Equals => left == right,
      ComparatorOperation::NotEquals => left != right,
      ComparatorOperation::LessThan => left < right,
      ComparatorOperation::LessThanOrEqualTo => left <= right,
      ComparatorOperation::GreaterThan => left > right,
      ComparatorOperation::GreaterThanOrEqualTo => left >= right,
    }
  }
}

impl crate::puzzle::PuzzleAsset for ComparatorAsset {
  fn create(self: &Self, _: &chrono::DateTime<chrono::Utc>) -> Box<dyn crate::puzzle::PuzzlePiece> {
    match self.value_type {
      ComparatorType::Bool => Box::new(Comparator { left: false, right: false, operation: self.operation }) as Box<dyn crate::puzzle::PuzzlePiece>,
      ComparatorType::Int => Box::new(Comparator { left: 0, right: 0, operation: self.operation }) as Box<dyn crate::puzzle::PuzzlePiece>,
    }
  }
  fn load<'a>(self: &Self, input: &mut crate::puzzle::InputBuffer, _: &chrono::DateTime<chrono::Utc>) -> crate::puzzle::DeserializationResult<'a> {
    match self.value_type {
      ComparatorType::Bool => load::<bool, crate::puzzle::InputBuffer>(input, self.operation),
      ComparatorType::Int => load::<u32, crate::puzzle::InputBuffer>(input, self.operation),
    }
  }
}

fn load<'a, T, R>(input: &mut crate::puzzle::InputBuffer, operation: ComparatorOperation) -> crate::puzzle::DeserializationResult<'a>
where
  T: 'a,
  T: Copy,
  T: Send + Sync,
  Vec<T>: Into<crate::puzzle::PieceValue>,
  T: Into<crate::puzzle::PieceValue>,
  T: crate::puzzle::EncodeValue,
  T: crate::puzzle::DecodeSaved,
  T: PartialOrd,
  T: crate::puzzle::ExtractValue,
{
  crate::puzzle::check_length(input, SERIALIZATION_LENGTH)?;
  Ok(Box::new(Comparator { left: T::read(input)?, right: T::read(input)?, operation }) as Box<dyn crate::puzzle::PuzzlePiece>)
}

impl<T> crate::puzzle::PuzzlePiece for Comparator<T>
where
  T: Copy,
  T: Send + Sync,
  T: PartialOrd,
  T: EncodeValue,
  T: crate::puzzle::ExtractValue,
{
  fn accept(self: &mut Self, name: &puzzleverse_core::PuzzleCommand, value: &crate::puzzle::PieceValue) -> crate::puzzle::OutputEvents {
    let old_state = self.operation.compare(self.left, self.right);

    let update = T::extract_value(value)
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

    let new_state = self.operation.compare(self.left, self.right);
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
